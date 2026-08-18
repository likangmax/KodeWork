#![forbid(unsafe_code)]

//! SSH local port forwarding: direct-tcpip tunnels from a local
//! loopback port to a remote host:port through the SSH transport.
//! Each tunnel has a bounded lifecycle (Creating -> Listening ->
//! Closed/Failed), is cancellable, and forwards traffic with
//! bidirectional streaming (never buffering whole streams).

use kodework_domain::{HostId, TunnelId};
use kodework_ssh::connection::SshConnection;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TunnelState {
    Creating,
    Listening,
    Closed,
    Failed,
}

/// Public snapshot of one tunnel.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TunnelInfo {
    pub id: TunnelId,
    pub host_id: HostId,
    pub local_addr: String,
    pub remote_host: String,
    pub remote_port: u16,
    pub state: TunnelState,
    pub active_connections: u32,
    pub error: Option<String>,
}

struct TunnelRuntime {
    info: Arc<Mutex<TunnelInfo>>,
    token: CancellationToken,
    accept_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    connections: Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>>,
    active_connections: Arc<AtomicU32>,
}

/// Clone-safe tunnel registry for one SessionManager.
#[derive(Clone)]
pub struct TunnelManager {
    tunnels: Arc<Mutex<HashMap<TunnelId, Arc<TunnelRuntime>>>>,
}

impl TunnelManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tunnels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Opens a loopback listener on `local_port` (0 picks a free port)
    /// and forwards every accepted connection to
    /// `remote_host:remote_port` over the SSH transport.
    pub async fn open(
        &self,
        connection: Arc<SshConnection>,
        host_id: HostId,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<TunnelInfo, String> {
        if remote_host.trim().is_empty() || remote_port == 0 {
            return Err(
                "remote host must not be empty and remote port must be non-zero".to_string(),
            );
        }
        if remote_host.chars().any(char::is_control) {
            return Err("remote host must not contain control characters".to_string());
        }
        let bind: SocketAddr = format!("127.0.0.1:{local_port}")
            .parse()
            .map_err(|error| format!("invalid local port: {error}"))?;
        let listener = TcpListener::bind(bind)
            .await
            .map_err(|error| format!("bind {bind} failed: {error}"))?;
        let actual_addr = listener
            .local_addr()
            .map_err(|error| format!("local_addr failed: {error}"))?;
        let id = TunnelId::new();
        let token = CancellationToken::new();
        let active_connections = Arc::new(AtomicU32::new(0));
        let info = TunnelInfo {
            id,
            host_id,
            local_addr: actual_addr.to_string(),
            remote_host: remote_host.to_string(),
            remote_port,
            state: TunnelState::Listening,
            active_connections: 0,
            error: None,
        };
        let runtime = Arc::new(TunnelRuntime {
            info: Arc::new(Mutex::new(info.clone())),
            token: token.clone(),
            accept_task: Mutex::new(None),
            connections: Arc::new(Mutex::new(HashMap::new())),
            active_connections: Arc::clone(&active_connections),
        });

        let accept_task = tokio::spawn(accept_loop(Arc::clone(&runtime), connection, listener));
        if let Ok(mut guard) = runtime.info.lock() {
            guard.active_connections = 0;
        }
        *runtime
            .accept_task
            .lock()
            .map_err(|_| "tunnel task lock poisoned".to_string())? = Some(accept_task);
        if let Ok(mut guard) = self.tunnels.lock() {
            guard.insert(id, Arc::clone(&runtime));
        }
        Ok(info)
    }

    /// Cancels the tunnel: stops accepting, closes the listener and all
    /// active connection tasks. Idempotent.
    pub async fn close(&self, tunnel_id: TunnelId) -> Result<(), String> {
        let runtime = {
            let guard = self
                .tunnels
                .lock()
                .map_err(|_| "tunnel registry poisoned".to_string())?;
            guard.get(&tunnel_id).cloned()
        };
        let Some(runtime) = runtime else {
            return Ok(());
        };
        runtime.token.cancel();
        let task = runtime
            .accept_task
            .lock()
            .map_err(|_| "tunnel task lock poisoned".to_string())?
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
        let mut conn_guard = runtime
            .connections
            .lock()
            .map_err(|_| "connection registry poisoned".to_string())?;
        for (_, task) in conn_guard.drain() {
            task.abort();
        }
        drop(conn_guard);
        if let Ok(mut guard) = runtime.info.lock() {
            guard.state = TunnelState::Closed;
            guard.active_connections = 0;
        }
        Ok(())
    }

    /// Closes every tunnel belonging to a host (used on reconnect).
    pub async fn close_all_for_host(&self, host_id: HostId) {
        let ids: Vec<TunnelId> = self
            .tunnels
            .lock()
            .map(|guard| {
                guard
                    .iter()
                    .filter(|(_, runtime)| {
                        runtime
                            .info
                            .lock()
                            .map(|info| info.host_id == host_id)
                            .unwrap_or(false)
                    })
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.close(id).await;
        }
        // Reap every tunnel of this host: after a reconnect the old
        // transport is gone and its tunnels can never be useful again.
        // (Manually closed tunnels stay listed as Closed until then.)
        if let Ok(mut guard) = self.tunnels.lock() {
            guard.retain(|_, runtime| {
                runtime
                    .info
                    .lock()
                    .map(|info| info.host_id != host_id)
                    .unwrap_or(true)
            });
        }
    }

    /// Snapshot of all tunnels (sorted by local port).
    #[must_use]
    pub fn list(&self) -> Vec<TunnelInfo> {
        let mut out: Vec<TunnelInfo> = self
            .tunnels
            .lock()
            .map(|guard| {
                guard
                    .values()
                    .filter_map(|runtime| {
                        reap_finished_connections(runtime);
                        runtime.info.lock().ok().map(|mut info| {
                            // The info snapshot must reflect live connection
                            // counts maintained by the proxy tasks.
                            info.active_connections =
                                runtime.active_connections.load(Ordering::SeqCst);
                            info.clone()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| a.local_addr.cmp(&b.local_addr));
        out
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn accept_loop(
    runtime: Arc<TunnelRuntime>,
    connection: Arc<SshConnection>,
    listener: TcpListener,
) {
    let mut next_conn: u64 = 0;
    loop {
        tokio::select! {
            _ = runtime.token.cancelled() => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((socket, _peer)) => {
                        let id = next_conn;
                        next_conn = next_conn.wrapping_add(1);
                        match connection
                            .forward_channel(
                                &runtime
                                    .info
                                    .lock()
                                    .map(|info| info.remote_host.clone())
                                    .unwrap_or_default(),
                                u32::from(
                                    runtime
                                        .info
                                        .lock()
                                        .map(|info| info.remote_port)
                                        .unwrap_or(0),
                                ),
                            )
                            .await
                        {
                            Ok(channel) => {
                                let channel_id = channel.id().number();
                                runtime
                                    .active_connections
                                    .fetch_add(1, Ordering::SeqCst);
                                let task = tokio::spawn({
                                    let runtime = Arc::clone(&runtime);
                                    let connection = Arc::clone(&connection);
                                    async move {
                                        proxy_connection(
                                            socket,
                                            channel,
                                            runtime.token.clone(),
                                            Arc::clone(&runtime.active_connections),
                                        )
                                        .await;
                                        connection.release_filtered_channel(channel_id);
                                        // Drop the finished task handle so
                                        // the registry only holds live
                                        // connections.
                                        if let Ok(mut guard) = runtime.connections.lock() {
                                            guard.remove(&id);
                                        }
                                    }
                                });
                                if let Ok(mut guard) = runtime.connections.lock() {
                                    guard.insert(id, task);
                                }
                                reap_finished_connections(&runtime);
                            }
                            Err(error) => {
                                if let Ok(mut guard) = runtime.info.lock() {
                                    guard.error = Some(format!("forward failed: {error}"));
                                    guard.state = TunnelState::Failed;
                                }
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        if let Ok(mut guard) = runtime.info.lock() {
                            guard.state = TunnelState::Failed;
                            guard.error = Some(format!("accept failed: {error}"));
                        }
                        break;
                    }
                }
            }
        }
    }
}

async fn proxy_connection(
    local: TcpStream,
    channel: kodework_ssh::connection::ForwardChannel,
    token: CancellationToken,
    active_connections: Arc<AtomicU32>,
) {
    let remote = channel.into_stream();
    let (mut local_r, mut local_w) = tokio::io::split(local);
    let (mut remote_r, mut remote_w) = tokio::io::split(remote);
    tokio::select! {
        // Half-close propagation: when one side reaches EOF, shut down the
        // other side's write half so the connection drains instead of
        // hanging on a keep-alive peer (e.g. an SSH server that never
        // closes direct-tcpip channels by itself).
        result = tokio::io::copy(&mut local_r, &mut remote_w) => {
            let _ = result;
            let _ = remote_w.shutdown().await;
        }
        result = tokio::io::copy(&mut remote_r, &mut local_w) => {
            let _ = result;
            let _ = local_w.shutdown().await;
        }
        _ = token.cancelled() => {
            {}
        }
    }
    active_connections.fetch_sub(1, Ordering::SeqCst);
}

/// Join handles can finish before the accept loop gets a chance to remove
/// them from the map (the task is allowed to run immediately after spawn).
/// Reaping on every snapshot closes that race and keeps the registry bounded
/// even when a peer connects and disconnects in the same scheduler tick.
fn reap_finished_connections(runtime: &TunnelRuntime) {
    if let Ok(mut guard) = runtime.connections.lock() {
        guard.retain(|_, task| !task.is_finished());
    }
}
