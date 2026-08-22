#![forbid(unsafe_code)]

//! Connection/session manager: address fallback, generation-guarded event
//! streams, pane routing, and PTY control. Automatic reconnection is
//! driven by the renderer (bounded, with backoff) because the core keeps
//! no credential material.

use crate::tunnel::{TunnelInfo, TunnelManager};
use kodework_domain::{
    validate_remote_path, Action, ActionMode, ConnectionState, Host, HostId, TransferDirection,
    TransferId,
};
use kodework_herdr::cli::{ExecOutput, HerdrClient, RemoteExecutor};
use kodework_herdr::HerdrError;
use kodework_network::CandidateResolver;
use kodework_sftp::backend::{RemoteFileMeta, RusshSftpBackend, SftpBackend};
use kodework_sftp::manager::{TransferEvent, TransferLeaseRegistry, TransferManager};
use kodework_sftp::{TransferRequest, DEFAULT_MAX_CONCURRENCY};
use kodework_ssh::connection::{
    AuthMethod, CommandOutput, ConnectionOptions, JumpSpec, ProxyCommand, SshConnection, SshPty,
};
use kodework_ssh::handler::SessionEvent;
use kodework_ssh::host_key::HostKeyBroker;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// Default size of the per-session event stream (bounded; backpressure).
pub const DEFAULT_EVENT_BUFFER: usize = 512;
/// Default connect deadline per candidate address.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default timeout for one remote command run.
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Default capture cap for one remote command (16 MiB).
pub const DEFAULT_CAPTURE_CAP: usize = 16 * 1024 * 1024;
/// Maximum one-shot PTY input payload accepted from IPC. Large pastes should
/// be chunked by the renderer rather than creating an unbounded allocation in
/// the native command boundary.
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Hard ceiling for local PTY panes on one SSH transport. This prevents a
/// renderer loop or malformed IPC caller from exhausting remote MaxSessions
/// and local channel state.
/// Keep the user-requested 20-terminal workflow bounded while leaving a
/// little headroom for future non-interactive channels.
pub const MAX_PANES_PER_HOST: usize = 20;
/// Initial PTY output can arrive between `open_pane` and the renderer's IPC
/// subscription. Keep a small bounded replay window per channel so login
/// banners and the first shell prompt are not lost.
const MAX_PENDING_PANE_BYTES: usize = 256 * 1024;
const MAX_PENDING_PANE_EVENTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    Connected { host_id: HostId, generation: u64 },
    Failed { host_id: HostId, reason: String },
}

/// Manages one SSH session per host.
#[derive(Clone)]
pub struct SessionManager {
    host_key: Arc<HostKeyBroker>,
    tunnels: TunnelManager,
    resolver: CandidateResolver,
    sessions: Arc<Mutex<HashMap<HostId, ActiveSession>>>,
    event_buffer: usize,
    connect_timeout: Duration,
    transfer_leases: TransferLeaseRegistry,
}

/// A session-event subscription: optional channel filter plus sender.
type EventSubscriber = (Option<u32>, mpsc::Sender<SessionEvent>);

#[derive(Default)]
struct PendingPaneEvents {
    events: VecDeque<SessionEvent>,
    bytes: usize,
}

/// One per-host transfer manager plus its event pump wiring.
struct TransferSlot {
    manager: Arc<TransferManager>,
    subscribers: Arc<Mutex<Vec<mpsc::Sender<TransferEvent>>>>,
    dropped_events: Arc<AtomicU64>,
}

struct ActiveSession {
    state: Arc<Mutex<ConnectionState>>,
    generation: Arc<AtomicU64>,
    /// Serializes connect/disconnect transitions for one host. Without a
    /// per-host async gate, two IPC callers can reserve the same generation
    /// and race to install different transports.
    connect_guard: Arc<tokio::sync::Mutex<()>>,
    connection: Arc<Mutex<Option<Arc<SshConnection>>>>,
    /// Split panes: pane id -> PTY channel (multiple terminals per
    /// session, e.g. multi-pane layouts without tmux).
    panes: Arc<Mutex<HashMap<u32, Arc<SshPty>>>>,
    next_pane: Arc<AtomicU32>,
    /// Lazily opened SFTP subsystem for this transport generation.
    sftp: Arc<Mutex<Option<Arc<russh_sftp::client::SftpSession>>>>,
    transfers: Arc<Mutex<Option<TransferSlot>>>,
    /// (channel filter, sender): `None` receives everything.
    subscribers: Arc<Mutex<Vec<EventSubscriber>>>,
    /// Bounded output received before a pane-specific renderer subscription.
    pending_events: Arc<Mutex<HashMap<u32, PendingPaneEvents>>>,
    dropped_events: Arc<AtomicU64>,
    herdr_bridges: Arc<Mutex<HashMap<u16, Arc<kodework_ssh::connection::SshExec>>>>,
}

impl Clone for ActiveSession {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            generation: Arc::clone(&self.generation),
            connect_guard: Arc::clone(&self.connect_guard),
            connection: Arc::clone(&self.connection),
            panes: Arc::clone(&self.panes),
            next_pane: Arc::clone(&self.next_pane),
            sftp: Arc::clone(&self.sftp),
            transfers: Arc::clone(&self.transfers),
            subscribers: Arc::clone(&self.subscribers),
            pending_events: Arc::clone(&self.pending_events),
            dropped_events: Arc::clone(&self.dropped_events),
            herdr_bridges: Arc::clone(&self.herdr_bridges),
        }
    }
}

impl SessionManager {
    #[must_use]
    pub fn new(
        host_key: Arc<HostKeyBroker>,
        resolver: CandidateResolver,
        event_buffer: usize,
    ) -> Self {
        Self {
            host_key,
            tunnels: TunnelManager::new(),
            resolver,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            event_buffer: event_buffer.max(8),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            transfer_leases: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Connects to the best candidate address, falling back on network
    /// failures only (auth/host-key failures are fatal).
    pub async fn connect(
        &self,
        host: &Host,
        auth: Vec<AuthMethod>,
    ) -> Result<SessionOutcome, String> {
        // Kept for library compatibility. The desktop command path uses
        // `connect_with_jump_auth` whenever a bastion has its own credential.
        self.connect_with_jump_auth(host, auth, None).await
    }

    /// Connects with explicitly separate target and jump-host credentials.
    /// `None` is retained only for older library callers that have no jump
    /// credential model yet; the Tauri production path passes `Some(...)` and
    /// therefore never reuses target credentials implicitly.
    pub async fn connect_with_jump_auth(
        &self,
        host: &Host,
        auth: Vec<AuthMethod>,
        jump_auth: Option<Vec<AuthMethod>>,
    ) -> Result<SessionOutcome, String> {
        self.ensure_session(host.id);
        let connect_guard = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host.id)
            .cloned()
            .ok_or_else(|| "session was not created".to_string())?
            .connect_guard;
        let _connect_guard = connect_guard.lock().await;
        self.set_state(host.id, ConnectionState::ResolvingAddress);
        let candidates = self.resolver.candidates(host).await;
        if candidates.is_empty() {
            self.set_state(host.id, ConnectionState::Failed);
            return Err("no enabled address candidates".into());
        }
        let generation = self.next_generation(host.id);
        let jump_auth = jump_auth.unwrap_or_else(|| auth.clone());

        let mut last_error = String::from("no candidate attempted");
        for candidate in candidates {
            self.set_state(host.id, ConnectionState::Connecting);
            let mut options = ConnectionOptions::new(
                candidate.address.hostname_or_ip.clone(),
                candidate.address.port,
                host.username.clone(),
                auth.clone(),
                Arc::clone(&self.host_key),
                generation,
            );
            options.logical_host_id = Some(host.id);
            options.connect_timeout = self.connect_timeout;
            if host.jump.is_none() {
                if let Some(proxy) = candidate.proxy {
                    options.proxy = Some(ProxyCommand {
                        program: std::path::PathBuf::from(proxy.program),
                        args: proxy.args,
                    });
                }
            }
            if let Some(jump) = &host.jump {
                options.jump = Some(JumpSpec {
                    hostname: jump.hostname.clone(),
                    port: jump.port,
                    username: jump.username.clone(),
                    auth: jump_auth.clone(),
                });
            }
            match SshConnection::connect(options).await {
                Ok((connection, events)) => {
                    self.attach(host.id, connection, events, generation);
                    self.set_state(host.id, ConnectionState::Ready);
                    return Ok(SessionOutcome::Connected {
                        host_id: host.id,
                        generation,
                    });
                }
                Err(error) => {
                    last_error = error.to_string();
                    if !kodework_ssh::address_fallback_is_retryable(&error) {
                        self.set_state(host.id, ConnectionState::Failed);
                        return Err(format!(
                            "fatal connection error for {}: {}",
                            candidate.address.hostname_or_ip, last_error
                        ));
                    }
                    // Network-class failures continue to the next candidate.
                }
            }
        }
        self.set_state(host.id, ConnectionState::Failed);
        Err(format!("all candidates failed: {last_error}"))
    }

    /// Subscribes to session events. When `channel` is `Some`, only
    /// events for that SSH channel are delivered (split-pane routing);
    /// `None` subscribes to everything (legacy single-terminal view).
    /// Each filter gets one reliable (backpressured) stream; extra
    /// subscribers for the same filter are best-effort mirrors.
    pub fn subscribe(
        &self,
        host_id: HostId,
        channel: Option<u32>,
    ) -> Option<mpsc::Receiver<SessionEvent>> {
        let session = self.sessions.lock().ok()?.get(&host_id).cloned()?;
        // Lock order matches `pump_events`: subscribers, then pending. This
        // makes replay draining atomic with live registration, so an event
        // cannot fall into the gap between those operations.
        let mut subscribers = session.subscribers.lock().ok()?;
        let mut pending = session.pending_events.lock().ok()?;
        let replay = channel
            .and_then(|channel| pending.remove(&channel))
            .map(|entry| entry.events)
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel(self.event_buffer.max(replay.len() + 8));
        for event in replay {
            if tx.try_send(event).is_err() {
                session.dropped_events.fetch_add(1, Ordering::SeqCst);
            }
        }
        subscribers.push((channel, tx));
        Some(rx)
    }

    #[must_use]
    pub fn state(&self, host_id: HostId) -> ConnectionState {
        let Some(session) = self
            .sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&host_id).cloned())
        else {
            return ConnectionState::Disconnected;
        };
        session
            .state
            .lock()
            .map(|guard| *guard)
            .unwrap_or(ConnectionState::Disconnected)
    }

    /// Marks a transport as waiting for fresh user input. The native
    /// supervisor uses this instead of retrying a credential failure forever.
    pub fn mark_waiting_for_credential(&self, host_id: HostId) {
        self.set_state(host_id, ConnectionState::WaitingForCredential);
    }

    /// Keeps a transient network failure under native supervisor ownership so
    /// the next supervisor tick can make another bounded attempt. This is
    /// intentionally separate from `WaitingForCredential` and `Failed`.
    pub fn mark_reconnecting(&self, host_id: HostId) {
        self.set_state(host_id, ConnectionState::Reconnecting);
    }

    #[must_use]
    pub fn generation(&self, host_id: HostId) -> u64 {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&host_id)
                    .map(|session| session.generation.load(Ordering::SeqCst))
            })
            .unwrap_or(0)
    }

    /// Sends terminal input to a split pane (bounded by russh's own flow
    /// control; no per-character IPC here).
    pub async fn send_input(
        &self,
        host_id: HostId,
        pane_id: u32,
        bytes: &[u8],
    ) -> Result<(), String> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(format!(
                "terminal input exceeds {} MiB",
                MAX_INPUT_BYTES / (1024 * 1024)
            ));
        }
        let pty = self.pane_pty(host_id, pane_id)?;
        pty.write(bytes).await.map_err(|error| error.to_string())
    }

    pub async fn resize(
        &self,
        host_id: HostId,
        pane_id: u32,
        cols: u32,
        rows: u32,
    ) -> Result<(), String> {
        // Guard against degenerate sizes: a hidden/zeroed terminal would
        // otherwise send an invalid window-change to the remote PTY.
        let cols = cols.clamp(2, 512);
        let rows = rows.clamp(2, 512);
        let pty = self.pane_pty(host_id, pane_id)?;
        pty.resize(cols, rows)
            .await
            .map_err(|error| error.to_string())
    }

    /// Opens a PTY shell on the session and registers it as a split
    /// pane. Returns (pane id, ssh channel id) so the renderer can
    /// route events and input per pane.
    pub async fn open_pane(
        &self,
        host_id: HostId,
        cols: u32,
        rows: u32,
    ) -> Result<(u32, u32), String> {
        let cols = cols.clamp(2, 512);
        let rows = rows.clamp(2, 512);
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let connection = session
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "not connected".to_string())?;
        let pty = connection
            .open_pty(cols, rows)
            .await
            .map_err(|error| error.to_string())?;
        let channel_id = pty.channel_id();
        let pane_id = session.next_pane.fetch_add(1, Ordering::SeqCst);
        let rejected_pty = {
            let mut panes = session
                .panes
                .lock()
                .map_err(|_| "panes lock poisoned".to_string())?;
            if panes.len() >= MAX_PANES_PER_HOST {
                Some(pty)
            } else {
                panes.insert(pane_id, Arc::new(pty));
                None
            }
        };
        if let Some(pty) = rejected_pty {
            let _ = pty.close().await;
            session
                .pending_events
                .lock()
                .map_err(|_| "pending events lock poisoned".to_string())?
                .remove(&channel_id);
            return Err(format!(
                "a host may have at most {MAX_PANES_PER_HOST} open terminal panes"
            ));
        }
        Ok((pane_id, channel_id))
    }

    /// Closes a split pane (idempotent).
    pub fn close_pane(&self, host_id: HostId, pane_id: u32) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let removed = session
            .panes
            .lock()
            .map_err(|_| "panes lock poisoned".to_string())?
            .remove(&pane_id);
        if let Some(pty) = removed {
            session
                .pending_events
                .lock()
                .map_err(|_| "pending events lock poisoned".to_string())?
                .remove(&pty.channel_id());
        }
        Ok(())
    }

    fn pane_pty(&self, host_id: HostId, pane_id: u32) -> Result<Arc<SshPty>, String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let guard = session
            .panes
            .lock()
            .map_err(|_| "panes lock poisoned".to_string())?;
        guard
            .get(&pane_id)
            .cloned()
            .ok_or_else(|| format!("no pane {pane_id} for host"))
    }

    /// Resolves the first live pane (lowest id). Used by features that
    /// target "the terminal" without a specific pane id so they keep
    /// working after pane 0 is closed.
    fn first_pane_pty(&self, host_id: HostId) -> Result<Arc<SshPty>, String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let guard = session
            .panes
            .lock()
            .map_err(|_| "panes lock poisoned".to_string())?;
        let pane_id = guard
            .keys()
            .copied()
            .min()
            .ok_or_else(|| "没有打开的终端".to_string())?;
        guard
            .get(&pane_id)
            .cloned()
            .ok_or_else(|| format!("no pane {pane_id} for host"))
    }

    /// Runs one command to completion on the session transport and
    /// returns its bounded output. The exec channel is drained by the
    /// SSH layer, so neither the russh buffer nor this API can stall;
    /// the command times out and the channel is dropped on cancellation.
    pub async fn run_remote(
        &self,
        host_id: HostId,
        command: &str,
    ) -> Result<CommandOutput, String> {
        self.run_remote_with_timeout(host_id, command, DEFAULT_COMMAND_TIMEOUT)
            .await
    }

    /// Like [`SessionManager::run_remote`] with a caller-chosen
    /// deadline.
    pub async fn run_remote_with_timeout(
        &self,
        host_id: HostId,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, String> {
        let connection = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "not connected".to_string())?;
        connection
            .run_command(command, timeout, DEFAULT_CAPTURE_CAP)
            .await
            .map_err(|error| error.to_string())
    }

    /// Opens an SSH local port forward to `remote_host:remote_port`.
    /// `local_port` 0 picks a free loopback port.
    pub async fn open_tunnel(
        &self,
        host_id: HostId,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<TunnelInfo, String> {
        let connection = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "not connected".to_string())?;
        self.tunnels
            .open(connection, host_id, local_port, remote_host, remote_port)
            .await
    }

    /// Cancels a tunnel (idempotent).
    pub async fn close_tunnel(&self, tunnel_id: kodework_domain::TunnelId) -> Result<(), String> {
        self.tunnels.close(tunnel_id).await
    }

    /// Lists all tunnels with live connection counts.
    #[must_use]
    pub fn list_tunnels(&self) -> Vec<TunnelInfo> {
        self.tunnels.list()
    }
    /// Returns the lazily opened SFTP subsystem for the current
    /// transport generation (reused across calls; reset on reconnect).
    pub async fn sftp_session(
        &self,
        host_id: HostId,
    ) -> Result<Arc<russh_sftp::client::SftpSession>, String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        if let Some(existing) = session
            .sftp
            .lock()
            .map_err(|_| "sftp lock poisoned".to_string())?
            .clone()
        {
            return Ok(existing);
        }
        let connection = session
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "not connected".to_string())?;
        let sftp = connection.sftp().await.map_err(|error| error.to_string())?;
        {
            let mut guard = session
                .sftp
                .lock()
                .map_err(|_| "sftp lock poisoned".to_string())?;
            // Lost the creation race: another caller opened a subsystem
            // while we were awaiting; reuse theirs and drop ours.
            if let Some(existing) = guard.as_ref() {
                return Ok(Arc::clone(existing));
            }
            *guard = Some(Arc::clone(&sftp));
        }
        Ok(sftp)
    }

    /// Returns the per-host transfer manager (created on first use; the
    /// transfer event stream is always pumped so no transfer can stall).
    pub async fn sftp_manager(&self, host_id: HostId) -> Result<Arc<TransferManager>, String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        {
            let guard = session
                .transfers
                .lock()
                .map_err(|_| "transfer lock poisoned".to_string())?;
            if let Some(slot) = guard.as_ref() {
                return Ok(Arc::clone(&slot.manager));
            }
        }
        let sftp = self.sftp_session(host_id).await?;
        let backend: Arc<dyn SftpBackend> = Arc::new(RusshSftpBackend::new(sftp));
        let (manager, events) = TransferManager::new_with_leases(
            backend,
            DEFAULT_MAX_CONCURRENCY,
            512,
            Arc::clone(&self.transfer_leases),
            host_id.as_uuid().simple().to_string(),
        );
        let manager = Arc::new(manager);
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let dropped_events = Arc::new(AtomicU64::new(0));
        tokio::spawn(pump_transfer_events(
            events,
            Arc::clone(&subscribers),
            Arc::clone(&dropped_events),
        ));
        {
            let mut guard = session
                .transfers
                .lock()
                .map_err(|_| "transfer lock poisoned".to_string())?;
            // Lost the creation race: reuse the manager another caller
            // installed while we were opening the SFTP session. Our own
            // pump exits once its event channel closes (manager drop).
            if let Some(slot) = guard.as_ref() {
                return Ok(Arc::clone(&slot.manager));
            }
            *guard = Some(TransferSlot {
                manager: Arc::clone(&manager),
                subscribers,
                dropped_events,
            });
        }
        Ok(manager)
    }

    /// Subscribes to transfer events. The first subscription becomes the
    /// primary stream (backpressure); later ones are best-effort mirrors.
    pub fn subscribe_transfers(&self, host_id: HostId) -> Option<mpsc::Receiver<TransferEvent>> {
        let session = self.sessions.lock().ok()?.get(&host_id).cloned()?;
        let binding = session.transfers.lock().ok()?;
        let slot = binding.as_ref()?;
        let (tx, rx) = mpsc::channel(256);
        slot.subscribers.lock().ok()?.push(tx);
        Some(rx)
    }

    /// Lists a remote directory over SFTP.
    pub async fn sftp_list(
        &self,
        host_id: HostId,
        path: &str,
    ) -> Result<Vec<RemoteFileMeta>, String> {
        validate_remote_path(path).map_err(|error| error.to_string())?;
        let sftp = self.sftp_session(host_id).await?;
        let backend = RusshSftpBackend::new(sftp);
        let mut entries = backend
            .list(path)
            .await
            .map_err(|error| error.to_string())?;
        // Keep a pathological remote directory from flooding the IPC bridge
        // and the renderer. The UI advertises this same bounded page size.
        entries.truncate(5_000);
        entries.sort_by(|left, right| {
            right.is_dir.cmp(&left.is_dir).then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
        });
        Ok(entries)
    }

    /// Enqueues an upload through the transfer manager.
    pub async fn sftp_upload(
        &self,
        host_id: HostId,
        local_path: &str,
        remote_path: &str,
        resume: bool,
    ) -> Result<TransferId, String> {
        if !std::path::Path::new(local_path).is_absolute() {
            return Err("本地路径必须是绝对路径".to_string());
        }
        validate_remote_path(remote_path).map_err(|error| error.to_string())?;
        let manager = self.sftp_manager(host_id).await?;
        let request = TransferRequest {
            local_path: local_path.to_string(),
            remote_path: remote_path.to_string(),
            direction: TransferDirection::Upload,
            resume,
        };
        manager
            .enqueue(request, 2)
            .await
            .map_err(|error| error.to_string())
    }

    /// Uploads an asset and waits until the remote atomic rename has
    /// completed. Callers may safely expose the remote path after this
    /// method returns.
    pub async fn sftp_upload_and_wait(
        &self,
        host_id: HostId,
        local_path: &str,
        remote_path: &str,
    ) -> Result<TransferId, String> {
        if !std::path::Path::new(local_path).is_absolute() {
            return Err("本地路径必须是绝对路径".to_string());
        }
        validate_remote_path(remote_path).map_err(|error| error.to_string())?;
        let manager = self.sftp_manager(host_id).await?;
        let request = TransferRequest {
            local_path: local_path.to_string(),
            remote_path: remote_path.to_string(),
            direction: TransferDirection::Upload,
            resume: true,
        };
        manager
            .enqueue_and_wait(request, 2)
            .await
            .map_err(|error| error.to_string())
    }

    /// Enqueues a download through the transfer manager.
    pub async fn sftp_download(
        &self,
        host_id: HostId,
        remote_path: &str,
        local_path: &str,
        resume: bool,
    ) -> Result<TransferId, String> {
        if !std::path::Path::new(local_path).is_absolute() {
            return Err("本地路径必须是绝对路径".to_string());
        }
        validate_remote_path(remote_path).map_err(|error| error.to_string())?;
        let manager = self.sftp_manager(host_id).await?;
        let request = TransferRequest {
            local_path: local_path.to_string(),
            remote_path: remote_path.to_string(),
            direction: TransferDirection::Download,
            resume,
        };
        manager
            .enqueue(request, 2)
            .await
            .map_err(|error| error.to_string())
    }

    pub fn sftp_pause(&self, host_id: HostId, transfer_id: TransferId) -> Result<(), String> {
        let binding = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?
            .transfers
            .lock()
            .map_err(|_| "transfer lock poisoned".to_string())?
            .as_ref()
            .ok_or_else(|| "no transfer manager for host".to_string())?
            .manager
            .pause(transfer_id)
            .map_err(|error| error.to_string());
        binding
    }

    pub fn sftp_resume(&self, host_id: HostId, transfer_id: TransferId) -> Result<(), String> {
        let binding = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?
            .transfers
            .lock()
            .map_err(|_| "transfer lock poisoned".to_string())?
            .as_ref()
            .ok_or_else(|| "no transfer manager for host".to_string())?
            .manager
            .resume(transfer_id)
            .map_err(|error| error.to_string());
        binding
    }

    pub fn sftp_cancel(&self, host_id: HostId, transfer_id: TransferId) -> Result<(), String> {
        let binding = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?
            .transfers
            .lock()
            .map_err(|_| "transfer lock poisoned".to_string())?
            .as_ref()
            .ok_or_else(|| "no transfer manager for host".to_string())?
            .manager
            .cancel(transfer_id)
            .map_err(|error| error.to_string());
        binding
    }

    /// Number of transfer events dropped from mirror subscriptions.
    #[must_use]
    pub fn transfer_dropped_events(&self, host_id: HostId) -> u64 {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions.get(&host_id).and_then(|session| {
                    session.transfers.lock().ok().and_then(|guard| {
                        guard
                            .as_ref()
                            .map(|slot| slot.dropped_events.load(Ordering::SeqCst))
                    })
                })
            })
            .unwrap_or(0)
    }

    /// Bridges the remote herdr control socket to a local loopback port.
    /// The remote socat process is owned by a long-lived SSH exec channel;
    /// closing the local tunnel or transport therefore closes the owner and
    /// cannot leave a detached remote process behind.
    /// Requires socat on the remote (documented; herdr's own --remote
    /// mechanism is the alternative on managed setups).
    pub async fn herdr_bridge(
        &self,
        host_id: HostId,
        local_port: u16,
    ) -> Result<HerdrBridgeInfo, String> {
        let client = self.herdr_client(host_id);
        let Some(socket) = client
            .socket_path()
            .await
            .map_err(|error| error.to_string())?
        else {
            return Err("远程未发现 herdr socket（需 herdr server 正在运行）".to_string());
        };
        if !client
            .has_socat()
            .await
            .map_err(|error| error.to_string())?
        {
            return Err("远程需要 socat 才能桥接 herdr socket（apt install socat）".to_string());
        }
        // Stable per-host remote port so the bridge can be identified without
        // persisting a remote PID.
        let remote_port = 28000u16 + u16::try_from(host_id.as_uuid().as_u128() % 2000).unwrap_or(0);
        let socket_path = socket;
        let quoted_socket = shell_quote(&socket_path)?;
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let connection = session
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "not connected".to_string())?;
        let owner = Arc::new(
            connection
                .exec(
                    &format!(
                        "exec socat TCP-LISTEN:{remote_port},bind=127.0.0.1,reuseaddr,fork UNIX-CONNECT:{quoted_socket}"
                    ),
                    false,
                    0,
                    0,
                )
                .await
                .map_err(|error| format!("socat 启动失败: {error}"))?,
        );
        let tunnel = match self
            .open_tunnel(host_id, local_port, "127.0.0.1", remote_port)
            .await
        {
            Ok(tunnel) => tunnel,
            Err(error) => {
                let _ = owner.close().await;
                return Err(error);
            }
        };
        let previous = session
            .herdr_bridges
            .lock()
            .map_err(|_| "bridge registry poisoned".to_string())?
            .insert(remote_port, Arc::clone(&owner));
        if let Some(previous) = previous {
            let _ = previous.close().await;
        }
        Ok(HerdrBridgeInfo {
            tunnel,
            remote_socket: socket_path,
            remote_port,
            // Kept for IPC compatibility with older renderers. Ownership is
            // now the SSH channel, not a remotely discovered PID.
            remote_pid: 0,
        })
    }

    /// Stops the remote socat bridge (idempotent). The local tunnel is
    /// closed separately via close_tunnel.
    pub async fn herdr_bridge_stop(
        &self,
        host_id: HostId,
        remote_port: u16,
        _remote_pid: Option<u32>,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let owner = session
            .herdr_bridges
            .lock()
            .map_err(|_| "bridge registry poisoned".to_string())?
            .remove(&remote_port);
        if let Some(owner) = owner {
            owner.close().await.map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Detects whether `yazi` is installed on the remote host.
    pub async fn yazi_available(&self, host_id: HostId) -> Result<bool, String> {
        let output = self
            .run_remote(
                host_id,
                "command -v yazi >/dev/null 2>&1 && echo yes || echo no",
            )
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout).contains("yes"))
    }

    /// Launches the yazi terminal file manager in the first live pane.
    pub async fn yazi_attach(&self, host_id: HostId) -> Result<(), String> {
        let pty = self.first_pane_pty(host_id)?;
        pty.write(b"\ryazi\r")
            .await
            .map_err(|error| error.to_string())
    }

    /// Runs an action: Quick/Background execute over an exec channel
    /// with bounded capture; Interactive sends the command into pane 0
    /// (the user drives it from the terminal). Dangerous actions must
    /// be confirmed first; the check happens here, not in the UI.
    pub async fn run_action(
        &self,
        host_id: HostId,
        action: &Action,
        confirmed: bool,
    ) -> Result<RunOutcome, String> {
        self.run_action_with_id(host_id, action, confirmed, None)
            .await
    }

    /// Runs an action with a caller-owned RunId. Persisted background runs use
    /// the same identity in SQLite, tmux and remote completion metadata.
    pub async fn run_action_with_id(
        &self,
        host_id: HostId,
        action: &Action,
        confirmed: bool,
        run_id: Option<kodework_domain::RunId>,
    ) -> Result<RunOutcome, String> {
        // Recompute the danger level server-side: the renderer-declared
        // field is only a hint and must never gate confirmation.
        let requires_confirmation = crate::action_requires_confirmation(action);
        if requires_confirmation && !confirmed {
            return Err("该动作需要确认后才能运行".to_string());
        }
        let command = build_action_command(action)?;
        match action.mode {
            ActionMode::Interactive => {
                let pty = self.first_pane_pty(host_id)?;
                let mut line = command;
                line.push('\r');
                pty.write(line.as_bytes())
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(RunOutcome {
                    disposition: RunDisposition::InteractiveDispatched,
                    exit_code: None,
                    stdout_preview: "interactive: 命令已发送到终端".to_string(),
                    stderr_preview: String::new(),
                    output_bytes: 0,
                    remote_session_ref: None,
                })
            }
            ActionMode::Quick => {
                let timeout = action
                    .timeout_ms
                    .map(Duration::from_millis)
                    .unwrap_or(DEFAULT_COMMAND_TIMEOUT);
                // Quick runs use the same atomic remote lifecycle markers as
                // background runs. The wrapper stays in the foreground so
                // normal quick commands still return captured output, while
                // a client timeout leaves enough evidence for reconciliation.
                // Library callers may execute a non-persisted Quick action.
                // Still give its remote lifecycle marker a unique identity so
                // concurrent calls cannot overwrite the nil/default run
                // directory used by older implementations.
                let run_id = match run_id {
                    Some(run_id) => run_id,
                    None => kodework_domain::RunId::new(),
                };
                let run_key = run_id.as_uuid().simple().to_string();
                let run_script = build_run_script(&command, &run_key)?;
                let output = self
                    .run_remote_with_timeout(host_id, &run_script, timeout)
                    .await?;
                let preview = |bytes: &[u8]| -> String {
                    let text = String::from_utf8_lossy(bytes);
                    let trimmed: String = text.trim().chars().take(400).collect();
                    trimmed
                };
                Ok(RunOutcome {
                    disposition: RunDisposition::Completed,
                    exit_code: output.exit_code,
                    stdout_preview: preview(&output.stdout),
                    stderr_preview: preview(&output.stderr),
                    output_bytes: (output.stdout.len() + output.stderr.len()) as u64,
                    remote_session_ref: Some(format!("metadata:{run_key}")),
                })
            }
            ActionMode::Background => {
                // A background action must survive UI/SSH disconnects. Run it
                // inside a detached tmux session and return the external
                // session name in the bounded preview for observability.
                let run_id = match run_id {
                    Some(run_id) => run_id,
                    None => kodework_domain::RunId::new(),
                };
                let session_name = format!("kodework-run-{}", run_id.as_uuid().simple());
                let run_key = run_id.as_uuid().simple().to_string();
                let run_script = build_run_script(&command, &run_key)?;
                let tmux_command = format!(
                    "tmux new-session -d -s {} -- sh -lc {}",
                    session_name,
                    shell_quote(&run_script)?
                );
                let output = self
                    .run_remote_with_timeout(host_id, &tmux_command, DEFAULT_COMMAND_TIMEOUT)
                    .await?;
                if output.exit_code != Some(0) {
                    return Ok(RunOutcome {
                        disposition: RunDisposition::Completed,
                        exit_code: output.exit_code,
                        stdout_preview: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                        stderr_preview: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        output_bytes: (output.stdout.len() + output.stderr.len()) as u64,
                        remote_session_ref: None,
                    });
                }
                Ok(RunOutcome {
                    disposition: RunDisposition::BackgroundStarted,
                    // tmux accepted the launcher; the user command has not
                    // finished yet. The caller must reconcile it later.
                    exit_code: None,
                    stdout_preview: format!("background tmux session: {session_name}"),
                    stderr_preview: String::new(),
                    output_bytes: 0,
                    remote_session_ref: Some(format!("tmux:{session_name}")),
                })
            }
        }
    }

    /// Reconciles one persisted Quick/Background run using authoritative
    /// remote metadata. Missing metadata is deliberately reported as Unknown.
    pub async fn reconcile_remote_run(
        &self,
        host_id: HostId,
        run_id: kodework_domain::RunId,
    ) -> Result<RemoteRunState, String> {
        let id = run_id.as_uuid().simple().to_string();
        let command = format!(
            "base=\"$HOME/.cache/kodework/runs/{id}\"; if [ -f \"$base/exit_code\" ]; then printf 'completed\\t%s\\t%s\\t%s\\n' \"$(cat -- \"$base/exit_code\")\" \"$(cat -- \"$base/started_at_s\" 2>/dev/null || true)\" \"$(cat -- \"$base/finished_at_s\" 2>/dev/null || true)\"; elif [ -f \"$base/started_at_s\" ] || tmux has-session -t 'kodework-run-{id}' >/dev/null 2>&1; then printf 'running\\n'; else printf 'unknown\\n'; fi"
        );
        let output = self.run_remote(host_id, &command).await?;
        let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let mut fields = line.split('\t');
        match fields.next() {
            Some("completed") => Ok(RemoteRunState::Completed {
                exit_code: fields
                    .next()
                    .and_then(|value| value.parse::<i32>().ok())
                    .ok_or_else(|| "remote run exit code is invalid".to_string())?,
                started_at_ms: parse_epoch_seconds(fields.next()),
                finished_at_ms: parse_epoch_seconds(fields.next()),
            }),
            Some("running") => Ok(RemoteRunState::Running),
            _ => Ok(RemoteRunState::Unknown),
        }
    }
    /// Lists remote tmux sessions (empty when tmux is unavailable).
    pub async fn tmux_list(&self, host_id: HostId) -> Result<Vec<TmuxSession>, String> {
        let output = self
            .run_remote(host_id, "tmux ls -F '#{session_name}\t#{session_windows}\t#{session_attached}\t#{session_created}' 2>/dev/null || true")
            .await?;
        let mut sessions = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split('\t');
            let (Some(name), Some(windows), Some(attached), Some(created)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            sessions.push(TmuxSession {
                name: name.to_string(),
                windows: windows.parse().unwrap_or(0),
                attached: attached.parse().unwrap_or(0),
                created: created.to_string(),
            });
        }
        sessions.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(sessions)
    }

    /// Creates a detached tmux session. Returns `Err` with a user-safe
    /// message when the name is taken or tmux is missing.
    pub async fn tmux_new(&self, host_id: HostId, name: &str) -> Result<(), String> {
        let safe = sanitize_tmux_name(name)?;
        let output = self
            .run_remote(host_id, &format!("tmux new-session -d -s {safe} 2>&1"))
            .await?;
        if output.exit_code == Some(0) {
            return Ok(());
        }
        let message = combined_output(&output);
        if message.contains("already exists") || message.contains("duplicate session") {
            return Err(format!("tmux session '{safe}' already exists"));
        }
        if message.contains("command not found") || message.contains("no such file") {
            return Err("tmux is not installed on the remote host".to_string());
        }
        Err(format!("tmux failed: {}", message.trim()))
    }

    /// Kills a remote tmux session. Missing sessions are treated as
    /// success (idempotent).
    pub async fn tmux_kill(&self, host_id: HostId, name: &str) -> Result<(), String> {
        let safe = sanitize_tmux_name(name)?;
        let output = self
            .run_remote(host_id, &format!("tmux kill-session -t {safe} 2>&1"))
            .await?;
        let message = combined_output(&output);
        if output.exit_code == Some(0) || message.contains("no such session") {
            return Ok(());
        }
        Err(format!("tmux kill failed: {}", message.trim()))
    }

    /// Builds a Herdr CLI client bound to this session. Commands run
    /// over a short-lived exec channel with timeouts and bounded
    /// capture (never through the PTY).
    #[must_use]
    pub fn herdr_client(&self, host_id: HostId) -> HerdrClient {
        HerdrClient::new(
            Box::new(SessionExecutor {
                manager: self.clone(),
                host_id,
            }),
            kodework_herdr::cli::DEFAULT_CLI_TIMEOUT,
        )
    }

    /// Focuses the PTY on the Herdr TUI (idempotent: sends a newline
    /// first so an already-running TUI just refreshes).
    pub async fn herdr_attach(&self, host_id: HostId) -> Result<(), String> {
        let pty = self.first_pane_pty(host_id)?;
        pty.write(b"\rherdr\r")
            .await
            .map_err(|error| error.to_string())
    }
    /// Requests a graceful disconnect. Remote tmux/Herdr sessions survive.
    pub async fn disconnect(&self, host_id: HostId) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "session registry poisoned".to_string())?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| "no session for host".to_string())?;
        let _connect_guard = session.connect_guard.lock().await;
        let connection = session
            .connection
            .lock()
            .map_err(|_| "connection lock poisoned".to_string())?
            .take();
        let disconnect_result = if let Some(connection) = connection {
            connection.disconnect().await.map_err(|e| e.to_string())
        } else {
            Ok(())
        };
        // Explicit disconnect is a full local teardown.  Keeping old PTY,
        // SFTP, transfer and subscriber handles here would retain the old
        // transport and let a later reconnect route events into stale UI.
        session
            .panes
            .lock()
            .map_err(|_| "panes lock poisoned".to_string())?
            .clear();
        *session
            .sftp
            .lock()
            .map_err(|_| "sftp lock poisoned".to_string())? = None;
        *session
            .transfers
            .lock()
            .map_err(|_| "transfers lock poisoned".to_string())? = None;
        let bridges = session
            .herdr_bridges
            .lock()
            .map_err(|_| "bridge registry poisoned".to_string())?
            .drain()
            .map(|(_, owner)| owner)
            .collect::<Vec<_>>();
        for owner in bridges {
            let _ = owner.close().await;
        }
        session
            .subscribers
            .lock()
            .map_err(|_| "subscribers lock poisoned".to_string())?
            .clear();
        session
            .pending_events
            .lock()
            .map_err(|_| "pending events lock poisoned".to_string())?
            .clear();
        self.tunnels.close_all_for_host(host_id).await;
        self.set_state(host_id, ConnectionState::Disconnected);
        disconnect_result
    }

    /// Reconnect state is owned by the desktop supervisor. The core keeps no
    /// credential material; the supervisor resolves secrets on each attempt.
    #[must_use]
    pub fn dropped_events(&self, host_id: HostId) -> u64 {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&host_id)
                    .map(|session| session.dropped_events.load(Ordering::SeqCst))
            })
            .unwrap_or(0)
    }

    /// Next connection generation: increments per (re)connect so stale',
    /// output from an older transport can be rejected.
    fn next_generation(&self, host_id: HostId) -> u64 {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&host_id).cloned())
            .map(|session| session.generation.load(Ordering::SeqCst) + 1)
            .unwrap_or(1)
    }

    fn ensure_session(&self, host_id: HostId) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions.entry(host_id).or_insert_with(|| ActiveSession {
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            generation: Arc::new(AtomicU64::new(0)),
            connect_guard: Arc::new(tokio::sync::Mutex::new(())),
            connection: Arc::new(Mutex::new(None)),
            panes: Arc::new(Mutex::new(HashMap::new())),
            next_pane: Arc::new(AtomicU32::new(0)),
            sftp: Arc::new(Mutex::new(None)),
            transfers: Arc::new(Mutex::new(None)),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            dropped_events: Arc::new(AtomicU64::new(0)),
            herdr_bridges: Arc::new(Mutex::new(HashMap::new())),
        });
    }

    fn attach(
        &self,
        host_id: HostId,
        connection: SshConnection,
        events: mpsc::Receiver<SessionEvent>,
        generation: u64,
    ) {
        let session = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&host_id)
            .cloned()
            .unwrap_or_else(|| unreachable!("session must exist"));
        *session
            .connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(connection));
        // A new transport invalidates any SFTP subsystem from the old one.
        *session
            .sftp
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        session
            .panes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        session
            .pending_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        // Tunnels from the old transport are useless; close them.
        let manager = self.tunnels.clone();
        tokio::spawn(async move { manager.close_all_for_host(host_id).await });
        let bridges = session
            .herdr_bridges
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
            .map(|(_, owner)| owner)
            .collect::<Vec<_>>();
        tokio::spawn(async move {
            for owner in bridges {
                let _ = owner.close().await;
            }
        });
        *session
            .transfers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        session.generation.store(generation, Ordering::SeqCst);
        self.set_state(host_id, ConnectionState::Connecting);

        let session_for_pump = Arc::new(session);
        tokio::spawn(async move {
            pump_events(session_for_pump, events, generation).await;
        });
    }

    fn set_state(&self, host_id: HostId, state: ConnectionState) {
        if let Ok(sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get(&host_id) {
                if let Ok(mut guard) = session.state.lock() {
                    *guard = state;
                }
            }
        }
    }
}

fn build_run_script(command: &str, run_key: &str) -> Result<String, String> {
    let quoted_command = shell_quote(command)?;
    Ok(format!(
        "umask 077 || exit 125; base=\"$HOME/.cache/kodework/runs/{run_key}\"; if ! mkdir -p -- \"$base\"; then exit 125; fi; started_tmp=\"$base/started_at_s.tmp.$$\"; if ! date +%s > \"$started_tmp\" || ! mv -f -- \"$started_tmp\" \"$base/started_at_s\"; then rm -f -- \"$started_tmp\"; exit 125; fi; sh -lc {quoted_command}; code=$?; finished_tmp=\"$base/finished_at_s.tmp.$$\"; exit_tmp=\"$base/exit_code.tmp.$$\"; if ! date +%s > \"$finished_tmp\" || ! mv -f -- \"$finished_tmp\" \"$base/finished_at_s\"; then rm -f -- \"$finished_tmp\" \"$exit_tmp\"; exit 125; fi; if ! printf '%s\\n' \"$code\" > \"$exit_tmp\" || ! mv -f -- \"$exit_tmp\" \"$base/exit_code\"; then rm -f -- \"$exit_tmp\"; exit 125; fi; exit \"$code\""
    ))
}

/// Forwards connection events to subscribers; the primary subscriber gets
/// true backpressure, mirrors use best-effort delivery with a drop counter.
async fn pump_events(
    session: Arc<ActiveSession>,
    mut events: mpsc::Receiver<SessionEvent>,
    generation: u64,
) {
    // One reliable (backpressured) sender per channel filter; extra
    // subscribers for the same filter are best-effort mirrors.
    let mut primaries: HashMap<Option<u32>, mpsc::Sender<SessionEvent>> = HashMap::new();
    while let Some(event) = events.recv().await {
        // A reconnect installs a new transport generation while an older
        // pump may still be draining its channel. Drop stale events before
        // they can reach subscribers or the bounded pending-event replay.
        if session.generation.load(Ordering::SeqCst) != generation {
            continue;
        }
        let subscribers = {
            let Ok(mut guard) = session.subscribers.lock() else {
                break;
            };
            // Drop subscribers whose receiver is gone; otherwise every
            // re-subscribe (reconnect, pane switch) would leak an entry
            // and the pump would keep probing dead senders forever.
            guard.retain(|(_, sender)| !sender.is_closed());
            if let Some(channel) = session_event_channel(&event) {
                let has_pane_subscriber = guard.iter().any(|(filter, _)| *filter == Some(channel));
                if !has_pane_subscriber {
                    if let Ok(mut pending) = session.pending_events.lock() {
                        push_pending_event(&mut pending, channel, event.clone());
                    }
                }
            }
            guard.clone()
        };
        // A cached primary whose receiver died must be dropped too, or a
        // fresh subscriber would never be promoted and would lose events.
        primaries.retain(|_, sender| !sender.is_closed());
        for (filter, subscriber) in subscribers {
            let matches = match (filter, &event) {
                (None, _) => true,
                (
                    Some(channel),
                    SessionEvent::Data {
                        channel: event_channel,
                        ..
                    },
                )
                | (
                    Some(channel),
                    SessionEvent::ExtendedData {
                        channel: event_channel,
                        ..
                    },
                )
                | (
                    Some(channel),
                    SessionEvent::ExitStatus {
                        channel: event_channel,
                        ..
                    },
                )
                | (
                    Some(channel),
                    SessionEvent::ExitSignal {
                        channel: event_channel,
                        ..
                    },
                )
                | (
                    Some(channel),
                    SessionEvent::ChannelClosed {
                        channel: event_channel,
                        ..
                    },
                ) => channel == *event_channel,
                (Some(_), _) => false,
            };
            if !matches {
                continue;
            }
            match primaries.get(&filter).cloned() {
                None => {
                    if subscriber.send(event.clone()).await.is_ok() {
                        primaries.insert(filter, subscriber);
                    }
                }
                Some(primary) if primary.same_channel(&subscriber) => {
                    // Keep the designated primary on the reliable,
                    // backpressured path.  Treating it as a mirror would
                    // silently drop terminal bytes when its queue is full.
                    if subscriber.send(event.clone()).await.is_err() {
                        primaries.remove(&filter);
                    }
                }
                Some(_) => {
                    if subscriber.try_send(event.clone()).is_err() {
                        session.dropped_events.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
        match &event {
            SessionEvent::Disconnected { .. } | SessionEvent::Error { .. } => {
                // Only the pump of the CURRENT transport may flip the
                // state; a stale pump from an older generation must not
                // overwrite a freshly connected Ready session.
                if session.generation.load(Ordering::SeqCst) != generation {
                    continue;
                }
                if let Ok(mut guard) = session.state.lock() {
                    if *guard == ConnectionState::Ready {
                        *guard = ConnectionState::Reconnecting;
                    }
                }
            }
            _ => {}
        }
    }
}

fn session_event_channel(event: &SessionEvent) -> Option<u32> {
    match event {
        SessionEvent::Data { channel, .. }
        | SessionEvent::ExtendedData { channel, .. }
        | SessionEvent::ExitStatus { channel, .. }
        | SessionEvent::ExitSignal { channel, .. }
        | SessionEvent::ChannelClosed { channel } => Some(*channel),
        SessionEvent::AuthBanner(_)
        | SessionEvent::Disconnected { .. }
        | SessionEvent::Error { .. } => None,
    }
}

fn session_event_size(event: &SessionEvent) -> usize {
    match event {
        SessionEvent::Data { bytes, .. } | SessionEvent::ExtendedData { bytes, .. } => bytes.len(),
        SessionEvent::ExitSignal { signal, .. } => signal.len(),
        SessionEvent::ExitStatus { .. } | SessionEvent::ChannelClosed { .. } => 1,
        SessionEvent::AuthBanner(value) => value.len(),
        SessionEvent::Disconnected { description } | SessionEvent::Error { description } => {
            description.len()
        }
    }
}

fn push_pending_event(
    pending: &mut HashMap<u32, PendingPaneEvents>,
    channel: u32,
    event: SessionEvent,
) {
    let size = session_event_size(&event);
    let entry = pending.entry(channel).or_default();
    entry.bytes = entry.bytes.saturating_add(size);
    entry.events.push_back(event);
    while entry.events.len() > MAX_PENDING_PANE_EVENTS || entry.bytes > MAX_PENDING_PANE_BYTES {
        let Some(removed) = entry.events.pop_front() else {
            break;
        };
        entry.bytes = entry.bytes.saturating_sub(session_event_size(&removed));
    }
}
/// A remote tmux session as reported by `tmux ls`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TmuxSession {
    pub name: String,
    pub windows: u32,
    pub attached: u32,
    pub created: String,
}

/// Restricts tmux session names to `[A-Za-z0-9_.-]` (1..=64 chars) so
/// they can be safely embedded in a shell command line.
pub fn sanitize_tmux_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("tmux session name must be 1..=64 characters".to_string());
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-'))
    {
        return Err(
            "tmux session name may only contain letters, digits, '_', '.' and '-'".to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Joins stdout and stderr of a command output for message matching
/// (a shell `2>&1` may place error text on either channel).
fn combined_output(output: &CommandOutput) -> String {
    let mut combined = String::from_utf8_lossy(&output.stderr).into_owned();
    if !combined.is_empty() && !output.stdout.is_empty() {
        combined.push(' ');
    }
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined
}
/// Runs herdr CLI commands over a session transport.
struct SessionExecutor {
    manager: SessionManager,
    host_id: HostId,
}

#[async_trait::async_trait]
impl RemoteExecutor for SessionExecutor {
    async fn exec(&self, command: &str, timeout: Duration) -> Result<ExecOutput, HerdrError> {
        let output = self
            .manager
            .run_remote_with_timeout(self.host_id, command, timeout)
            .await
            .map_err(HerdrError::Executor)?;
        Ok(ExecOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code.unwrap_or(-1),
        })
    }
}
/// Forwards transfer events to subscribers; the primary subscriber gets
/// true backpressure, mirrors use best-effort delivery with a drop
/// counter. Always running, so the manager never blocks on its event
/// buffer.
async fn pump_transfer_events(
    mut events: mpsc::Receiver<TransferEvent>,
    subscribers: Arc<Mutex<Vec<mpsc::Sender<TransferEvent>>>>,
    dropped_events: Arc<AtomicU64>,
) {
    let mut primary: Option<mpsc::Sender<TransferEvent>> = None;
    while let Some(event) = events.recv().await {
        let mirrors = {
            let Ok(mut guard) = subscribers.lock() else {
                break;
            };
            // Same leak guard as the session pump: re-subscribes must not
            // accumulate dead senders.
            guard.retain(|sender| !sender.is_closed());
            guard.clone()
        };
        let mut send_error = false;
        for subscriber in mirrors {
            match primary.as_ref() {
                None => {
                    if subscriber.send(event.clone()).await.is_err() {
                        send_error = true;
                    } else {
                        primary = Some(subscriber);
                    }
                }
                Some(current) if current.same_channel(&subscriber) => {
                    if subscriber.send(event.clone()).await.is_err() {
                        send_error = true;
                    }
                }
                Some(_) => {
                    if subscriber.try_send(event.clone()).is_err() {
                        dropped_events.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
        if send_error {
            primary = None;
        }
    }
}
/// Result of bridging the remote herdr socket to a local port.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HerdrBridgeInfo {
    pub tunnel: crate::tunnel::TunnelInfo,
    pub remote_socket: String,
    pub remote_port: u16,
    pub remote_pid: u32,
}
/// Result of running one action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunOutcome {
    pub disposition: RunDisposition,
    pub exit_code: Option<i32>,
    pub stdout_preview: String,
    pub stderr_preview: String,
    pub output_bytes: u64,
    pub remote_session_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RemoteRunState {
    Running,
    Completed {
        exit_code: i32,
        started_at_ms: Option<u64>,
        finished_at_ms: Option<u64>,
    },
    Unknown,
}

fn parse_epoch_seconds(value: Option<&str>) -> Option<u64> {
    value?.parse::<u64>().ok()?.checked_mul(1_000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RunDisposition {
    Completed,
    BackgroundStarted,
    InteractiveDispatched,
}

/// Quotes data fields before they are embedded in a POSIX remote shell
/// command. The action command itself remains intentionally raw because an
/// Action is explicitly a user-authored shell command; its danger level is
/// recomputed and confirmation is enforced by `run_action`.
fn shell_quote(value: &str) -> Result<String, String> {
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err("字段包含不允许的控制字符".to_string());
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn shell_cd(path: &str) -> Result<String, String> {
    validate_remote_path(path).map_err(|error| error.to_string())?;
    if let Some(relative) = path.strip_prefix("~/") {
        Ok(format!(
            "cd -- \"$HOME\" && cd -- {}",
            shell_quote(relative)?
        ))
    } else if path == "~" {
        Ok("cd -- \"$HOME\"".to_string())
    } else {
        Ok(format!("cd -- {}", shell_quote(path)?))
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod command_safety_tests {
    use super::*;
    use kodework_domain::{ActionId, ActionMode, ConfirmationPolicy, DangerLevel, ProjectId};

    fn action() -> Action {
        Action {
            id: ActionId::new(),
            project_id: ProjectId::new(),
            name: "test".into(),
            command: "printf ok".into(),
            mode: ActionMode::Quick,
            cwd: None,
            timeout_ms: None,
            danger_level: DangerLevel::Safe,
            confirmation: ConfirmationPolicy::Never,
            env: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn action_data_fields_are_shell_quoted() {
        let mut value = action();
        value.cwd = Some("/tmp/work; touch /tmp/should-not-run".into());
        value.env.insert("SAFE_NAME".into(), "a'b; echo no".into());
        let command = build_action_command(&value)
            .unwrap_or_else(|error| unreachable!("valid quoted action data should build: {error}"));
        assert!(command.contains("SAFE_NAME='a'\\''b; echo no'"));
        assert!(command.contains("cd -- '/tmp/work; touch /tmp/should-not-run'"));
    }

    #[test]
    fn invalid_environment_name_is_rejected() {
        let mut value = action();
        value.env.insert("BAD=NAME".into(), "value".into());
        assert!(build_action_command(&value).is_err());
    }

    #[test]
    fn control_characters_in_data_fields_are_rejected() {
        let mut value = action();
        value.cwd = Some("/tmp/line\nfeed".into());
        assert!(build_action_command(&value).is_err());
    }

    #[test]
    fn home_relative_cwd_expands_home_without_tilde_quoting() {
        let mut value = action();
        value.cwd = Some("~/workspace/project".into());
        let command = build_action_command(&value)
            .unwrap_or_else(|error| unreachable!("valid action should build: {error}"));
        assert!(command.contains("cd -- \"$HOME\" && cd -- 'workspace/project'"));
        assert!(!command.contains("'~/workspace/project'"));
    }

    #[test]
    fn remote_epoch_seconds_are_converted_without_overflow() {
        assert_eq!(
            parse_epoch_seconds(Some("1720000000")),
            Some(1_720_000_000_000)
        );
        assert_eq!(parse_epoch_seconds(Some("invalid")), None);
        assert_eq!(parse_epoch_seconds(None), None);
        assert_eq!(parse_epoch_seconds(Some(&u64::MAX.to_string())), None);
    }

    #[test]
    fn background_metadata_preflight_is_fail_closed() {
        let script = build_run_script("printf user-command", "run-123")
            .unwrap_or_else(|error| unreachable!("script should be quoted: {error}"));
        let mkdir = script.find("mkdir -p --").unwrap_or(usize::MAX);
        let command = script.find("sh -lc").unwrap_or(usize::MAX);
        assert!(mkdir < command, "metadata setup must precede user command");
        assert!(script.contains("if ! mkdir -p --"));
        assert!(script.contains("exit 125"), "metadata failures must abort");
        assert!(script.contains("started_at_s"));
        assert!(script.contains("finished_at_s"));
        assert!(script.contains("exit_code"));
    }

    #[tokio::test]
    async fn stale_generation_events_are_dropped_before_replay() {
        let (event_tx, event_rx) = mpsc::channel(2);
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(2);
        let session = Arc::new(ActiveSession {
            state: Arc::new(Mutex::new(ConnectionState::Ready)),
            generation: Arc::new(AtomicU64::new(2)),
            connect_guard: Arc::new(tokio::sync::Mutex::new(())),
            connection: Arc::new(Mutex::new(None::<Arc<SshConnection>>)),
            panes: Arc::new(Mutex::new(HashMap::new())),
            next_pane: Arc::new(AtomicU32::new(0)),
            sftp: Arc::new(Mutex::new(None::<Arc<russh_sftp::client::SftpSession>>)),
            transfers: Arc::new(Mutex::new(None::<TransferSlot>)),
            subscribers: Arc::new(Mutex::new(vec![(None, subscriber_tx)])),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            dropped_events: Arc::new(AtomicU64::new(0)),
            herdr_bridges: Arc::new(Mutex::new(HashMap::new())),
        });

        event_tx
            .send(SessionEvent::Data {
                channel: 7,
                bytes: b"stale transport output".to_vec(),
            })
            .await
            .unwrap_or_else(|error| unreachable!("send stale event: {error}"));
        drop(event_tx);
        pump_events(session.clone(), event_rx, 1).await;

        assert!(subscriber_rx.try_recv().is_err());
        assert!(session
            .pending_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty());
    }
}

/// Builds the remote shell line for an action: optional quoted env prefix,
/// optional quoted cwd, then the intentional raw command text.
fn build_action_command(action: &Action) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in &action.env {
        if !valid_env_key(key) {
            return Err(format!("环境变量名无效: {key}"));
        }
        parts.push(format!("{key}={}", shell_quote(value)?));
    }
    if let Some(cwd) = &action.cwd {
        parts.push(shell_cd(cwd)?);
    }
    parts.push(action.command.clone());
    Ok(parts.join(" && ").trim().to_string())
}
