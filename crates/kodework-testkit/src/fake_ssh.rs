#![forbid(unsafe_code)]

//! Fake SSH server for offline integration and fault tests.
//!
//! Capabilities: password/public-key authentication (configurable), PTY
//! echo shell, fixed exec responses, byte flood, delayed disconnect, and a
//! swappable host key to simulate key rotation (MITM detection).

use russh::keys::ssh_key::Algorithm;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, ChannelOpenHandle, Server, Session};
use russh::{Channel, ChannelId};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Behavior of the fake PTY shell after `channel_success`.
#[derive(Debug, Clone)]
pub enum FakeShellBehavior {
    /// Echo every input chunk back followed by CRLF.
    Echo,
    /// Write `bytes` of 0x61 ('a') immediately after the shell opens.
    Flood { bytes: usize },
    /// Keep the channel open for `delay`, then drop the transport
    /// (simulates a network drop).
    DropAfter { delay: Duration },
}

/// A scripted exec response: stdout, stderr, exit code.
#[derive(Debug, Clone)]
pub struct FakeExecResponse {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: u32,
}

/// Behavior of `exec` requests.
#[derive(Debug, Clone)]
pub enum FakeExecBehavior {
    /// Respond with fixed output, exit status, EOF and close.
    Fixed { output: Vec<u8>, exit_code: u32 },
    /// Respond with the also-valid EOF-before-exit-status ordering.
    EofBeforeExit { output: Vec<u8>, exit_code: u32 },
    /// Route each command through `script` (first matching prefix wins);
    /// unmatched commands get `fallback`.
    Scripted {
        script: Vec<(String, FakeExecResponse)>,
        fallback: FakeExecResponse,
    },
    /// Reject the exec request (channel failure).
    Reject,
}

/// Server options. All fields are shared by every connection.
#[derive(Debug, Clone)]
pub struct FakeSshOptions {
    /// When `Some`, only this password is accepted.
    pub password: Option<String>,
    /// Whether public-key authentication is accepted.
    pub accept_publickey: bool,
    /// Artificial delay before answering `auth_password` (timeout tests).
    pub auth_delay: Duration,
    pub shell: FakeShellBehavior,
    pub exec: FakeExecBehavior,
    /// Kept for future real-SFTP subsystem support; not yet served.
    pub sftp: Option<crate::fake_sftp_server::FakeSftpContent>,
    /// When set, direct-tcpip channels are bridged to this TCP target
    /// (jump-host mode) instead of echoing.
    pub jump_target: Option<std::net::SocketAddr>,
}

impl Default for FakeSshOptions {
    fn default() -> Self {
        Self {
            password: Some("test-password".into()),
            accept_publickey: false,
            auth_delay: Duration::ZERO,
            shell: FakeShellBehavior::Echo,
            exec: FakeExecBehavior::Fixed {
                output: b"ok\n".to_vec(),
                exit_code: 0,
            },
            sftp: None,
            jump_target: None,
        }
    }
}

/// A running fake SSH server bound to 127.0.0.1 on an ephemeral port.
pub struct FakeSshServer {
    options: Arc<FakeSshOptions>,
    host_key: Arc<PrivateKey>,
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl FakeSshServer {
    /// Starts a server with a fresh random Ed25519 host key.
    pub async fn start(options: FakeSshOptions) -> Result<Self, String> {
        let mut rng = rand::rng();
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .map_err(|error| format!("host key generation failed: {error}"))?;
        Self::start_with_key(options, key).await
    }

    /// Starts a server with a caller-provided host key (enables key-rotation
    /// tests: restart on the same port with a different key).
    pub async fn start_with_key(
        options: FakeSshOptions,
        host_key: PrivateKey,
    ) -> Result<Self, String> {
        Self::start_with_key_on_port(options, host_key, 0).await
    }

    /// Starts a server with a caller-provided host key on a concrete port;
    /// `0` picks an ephemeral port. A concrete port enables same-port
    /// key-rotation tests (restart with a different key).
    pub async fn start_with_key_on_port(
        options: FakeSshOptions,
        host_key: PrivateKey,
        port: u16,
    ) -> Result<Self, String> {
        let options = Arc::new(options);
        let host_key = Arc::new(host_key);
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|error| format!("bind failed: {error}"))?;
        let addr = listener
            .local_addr()
            .map_err(|error| format!("local_addr failed: {error}"))?;

        let config = Arc::new(server::Config {
            keys: vec![(*host_key).clone()],
            auth_rejection_time: Duration::from_millis(50),
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });

        let fake = FakeServer {
            options: Arc::clone(&options),
        };
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let mut fake = fake;
            let server_task = fake.run_on_socket(config, &listener);
            let server_handle = server_task.handle();
            tokio::select! {
                _ = shutdown_rx => {
                    server_handle.shutdown("fake server shutting down".into());
                },
                _ = server_task => {}
            }
        });

        Ok(Self {
            options,
            host_key,
            addr,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[must_use]
    pub fn host_key(&self) -> PublicKey {
        self.host_key.public_key().clone()
    }

    #[must_use]
    pub fn options(&self) -> &FakeSshOptions {
        &self.options
    }

    /// Starts a server with a fresh random key on a concrete port
    /// (host-key rotation tests).
    pub async fn start_with_new_key_on_port(
        options: FakeSshOptions,
        port: u16,
    ) -> Result<Self, String> {
        let mut rng = rand::rng();
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .map_err(|error| format!("host key generation failed: {error}"))?;
        Self::start_with_key_on_port(options, key, port).await
    }

    /// Stops the listener and all live connections.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Give the server task a moment to observe the shutdown.
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

struct FakeServer {
    options: Arc<FakeSshOptions>,
}

impl server::Server for FakeServer {
    type Handler = FakeHandler;

    fn new_client(&mut self, _addr: Option<SocketAddr>) -> Self::Handler {
        FakeHandler {
            options: Arc::clone(&self.options),
            direct_channels: std::collections::HashSet::new(),
            jump_tcp: std::collections::HashMap::new(),
        }
    }

    fn handle_session_error(&mut self, error: <Self::Handler as server::Handler>::Error) {
        eprintln!("fake ssh session error: {error}");
    }
}

struct FakeHandler {
    options: Arc<FakeSshOptions>,
    /// Channels opened as direct-tcpip forwards; their data is echoed
    /// verbatim (simulates a remote TCP service for tunnel tests).
    direct_channels: std::collections::HashSet<ChannelId>,
    /// Jump-mode bridges: channel -> target TCP write half.
    jump_tcp: std::collections::HashMap<ChannelId, tokio::net::tcp::OwnedWriteHalf>,
}

impl server::Handler for FakeHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        if !self.options.auth_delay.is_zero() {
            tokio::time::sleep(self.options.auth_delay).await;
        }
        let expected = self.options.password.as_deref();
        if expected.is_some_and(|value| value == password) {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, _user: &str, _key: &PublicKey) -> Result<Auth, Self::Error> {
        if self.options.accept_publickey {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<server::Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        _host_to_connect: &str,
        _port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        if let Some(target) = self.options.jump_target {
            let Ok(stream) = tokio::net::TcpStream::connect(target).await else {
                return Ok(());
            };
            let (mut read_handle, write_handle) = stream.into_split();
            self.jump_tcp.insert(channel.id(), write_handle);
            let handle = session.handle();
            let channel_id = channel.id();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buf = vec![0u8; 8192];
                loop {
                    match read_handle.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if handle.data(channel_id, buf[..n].to_vec()).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                let _ = handle.eof(channel_id).await;
            });
        } else {
            self.direct_channels.insert(channel.id());
        }
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        match self.options.shell {
            FakeShellBehavior::Echo => {}
            FakeShellBehavior::Flood { bytes } => {
                let chunk = vec![b'a'; bytes.min(64 * 1024)];
                let mut remaining = bytes;
                while remaining > 0 {
                    let send = remaining.min(chunk.len());
                    // session.data is synchronous; when the window is
                    // exhausted it errors. Retry after a tick so the
                    // client can drain and extend the window (true
                    // backpressure).
                    loop {
                        match session.data(channel, chunk[..send].to_vec()) {
                            Ok(()) => break,
                            Err(error) => {
                                eprintln!("flood data error: {error:?}");
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                        }
                    }
                    remaining -= send;
                }
            }
            FakeShellBehavior::DropAfter { delay } => {
                tokio::time::sleep(delay).await;
                return Err(russh::Error::Disconnect);
            }
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match &self.options.exec {
            FakeExecBehavior::Fixed { output, exit_code } => {
                session.channel_success(channel)?;
                if !output.is_empty() {
                    session.data(channel, output.clone())?;
                }
                session.exit_status_request(channel, *exit_code)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            FakeExecBehavior::EofBeforeExit { output, exit_code } => {
                session.channel_success(channel)?;
                if !output.is_empty() {
                    session.data(channel, output.clone())?;
                }
                session.eof(channel)?;
                session.exit_status_request(channel, *exit_code)?;
                session.close(channel)?;
            }
            FakeExecBehavior::Scripted { script, fallback } => {
                let command = String::from_utf8_lossy(_data);
                let response = script
                    .iter()
                    .find(|(prefix, _)| command.starts_with(prefix.as_str()))
                    .map(|(_, response)| response)
                    .unwrap_or(fallback);
                session.channel_success(channel)?;
                if !response.stdout.is_empty() {
                    session.data(channel, response.stdout.clone())?;
                }
                if !response.stderr.is_empty() {
                    session.extended_data(channel, 1, response.stderr.clone())?;
                }
                session.exit_status_request(channel, response.exit_code)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            FakeExecBehavior::Reject => {
                session.channel_failure(channel)?;
            }
        }
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // russh server does not expose client channels as streams, so
        // real SFTP traffic is tested with a duplex stream directly;
        // here we reject so client code fails fast instead of stalling.
        let _ = name;
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(stream) = self.jump_tcp.get_mut(&channel) {
            use tokio::io::AsyncWriteExt;
            let _ = stream.write_all(data).await;
            return Ok(());
        }
        if self.direct_channels.contains(&channel) {
            session.data(channel, data.to_vec())?;
            return Ok(());
        }
        match self.options.shell {
            FakeShellBehavior::Echo => {
                let mut echoed = data.to_vec();
                echoed.extend_from_slice(b"\r\n");
                session.data(channel, echoed)?;
            }
            FakeShellBehavior::Flood { .. } | FakeShellBehavior::DropAfter { .. } => {}
        }
        Ok(())
    }
}
