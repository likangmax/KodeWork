#![forbid(unsafe_code)]

//! SSH connection lifecycle: connect with deadline, ordered authentication,
//! PTY/exec channels and cancellation-aware shutdown.

use crate::handler::{SessionEvent, SshHandler};
use crate::host_key::HostKeyBroker;
use crate::SshError;
use kodework_domain::HostId;
use russh::client;
use russh::keys::agent::{client::AgentClient, AgentIdentity};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::Channel;
use std::collections::HashSet;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use zeroize::Zeroize;

/// Default channel event buffer (bounded; backpressure pauses the stream).
pub const DEFAULT_EVENT_BUFFER: usize = 256;
/// Default TCP connect deadline.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default keepalive interval when none is configured.
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// Credential material handed to the connection layer. Password bytes are
/// zeroized on drop via `zeroize::Zeroizing`.
#[derive(Clone)]
pub enum AuthMethod {
    /// Password / passphrase authentication.
    Password(ZeroizingVec),
    /// Public key from a local file; optional passphrase for encrypted keys.
    PublicKey {
        key_path: PathBuf,
        passphrase: Option<ZeroizingVec>,
    },
    /// Use identities exposed by Windows OpenSSH Agent or Pageant.
    SshAgent,
    /// RFC 4256 keyboard-interactive. The first prompt flow is supported via
    /// `KeyboardInteractive` with a responder; see `KeyboardResponder`.
    KeyboardInteractive {
        broker: Arc<crate::keyboard_interactive::KeyboardInteractiveBroker>,
    },
}

/// Zeroizing byte vector for credential material.
pub type ZeroizingVec = zeroize::Zeroizing<Vec<u8>>;

impl std::fmt::Debug for AuthMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::Password(_) => formatter.write_str("Password(REDACTED)"),
            AuthMethod::PublicKey { key_path, .. } => formatter
                .debug_struct("PublicKey")
                .field("key_path", key_path)
                .field("passphrase", &"REDACTED")
                .finish(),
            AuthMethod::SshAgent => formatter.write_str("SshAgent"),
            AuthMethod::KeyboardInteractive { .. } => formatter.write_str("KeyboardInteractive"),
        }
    }
}

/// Bastion host hop: the real SSH connection is chained through this
/// server using a direct-tcpip channel to the target ssh port.
#[derive(Debug, Clone)]
pub struct JumpSpec {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth: Vec<AuthMethod>,
}

/// External raw-stream transport (for example `tailscale nc host port`).
/// The child process is owned by the SSH connection and is killed when the
/// stream is dropped.
#[derive(Debug, Clone)]
pub struct ProxyCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Options for establishing one SSH connection.
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    /// Logical workstation identity used to bind trust across fallback paths.
    pub logical_host_id: Option<HostId>,
    /// Authentication methods tried in order until one succeeds.
    pub auth: Vec<AuthMethod>,
    pub host_key: Arc<HostKeyBroker>,
    pub connect_timeout: Duration,
    pub keepalive_interval: Option<Duration>,
    pub inactivity_timeout: Option<Duration>,
    pub event_buffer_size: usize,
    /// Generation tag for stale-output rejection after reconnect.
    pub generation: u64,
    /// Optional bastion hop (chain the connection through this host).
    pub jump: Option<JumpSpec>,
    /// Optional process-backed raw TCP transport.
    pub proxy: Option<ProxyCommand>,
}

impl ConnectionOptions {
    #[must_use]
    pub fn new(
        hostname: String,
        port: u16,
        username: String,
        auth: Vec<AuthMethod>,
        host_key: Arc<HostKeyBroker>,
        generation: u64,
    ) -> Self {
        Self {
            hostname,
            port,
            username,
            logical_host_id: None,
            auth,
            host_key,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
            inactivity_timeout: None,
            event_buffer_size: DEFAULT_EVENT_BUFFER,
            generation,
            jump: None,
            proxy: None,
        }
    }
}

/// Channel type used for direct-tcpip forwarding.
pub type ForwardChannel = russh::Channel<russh::client::Msg>;

/// A live SSH connection. Dropping it does not terminate the remote
/// session (tmux/Herdr own persistence); call `disconnect` explicitly.
pub struct SshConnection {
    handle: client::Handle<SshHandler>,
    generation: u64,
    filtered_channels: Arc<Mutex<HashSet<u32>>>,
    /// Keeps the bastion connection alive when connected through a jump host.
    _jump: Option<Box<SshConnection>>,
}

impl std::fmt::Debug for SshConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SshConnection")
            .field("generation", &self.generation)
            .finish()
    }
}

impl SshConnection {
    /// Connects (TCP + KEX + host-key check + authentication).
    ///
    /// Returns the connection and the bounded event stream carrying all
    /// channel data and lifecycle events. Dropping the receiver cancels
    /// the connection (the remote session itself survives via tmux/Herdr).
    pub async fn connect(
        options: ConnectionOptions,
    ) -> Result<(SshConnection, mpsc::Receiver<SessionEvent>), SshError> {
        if let Some(jump) = options.jump.clone() {
            if options.proxy.is_some() {
                return Err(SshError::InvalidConfiguration(
                    "proxy transport cannot be combined with a jump host".into(),
                ));
            }
            return Self::connect_via_jump(options, jump).await;
        }
        Self::connect_direct(options).await
    }

    /// Direct TCP + SSH to the target (no bastion).
    async fn connect_direct(
        options: ConnectionOptions,
    ) -> Result<(SshConnection, mpsc::Receiver<SessionEvent>), SshError> {
        if options.hostname.trim().is_empty() || options.port == 0 {
            return Err(SshError::InvalidConfiguration(
                "hostname must not be empty and port must be non-zero".into(),
            ));
        }
        if options.username.trim().is_empty() {
            return Err(SshError::InvalidConfiguration(
                "username must not be empty".into(),
            ));
        }
        if options.auth.is_empty() {
            return Err(SshError::InvalidConfiguration(
                "at least one auth method required".into(),
            ));
        }
        let event_buffer_size = options.event_buffer_size.max(8);

        let config = Arc::new(client::Config {
            inactivity_timeout: options.inactivity_timeout,
            keepalive_interval: options.keepalive_interval,
            // A larger receive window keeps SFTP and terminal bursts moving
            // over higher-latency Tailscale/DERP paths without changing SSH
            // packet compatibility.
            window_size: 8 * 1024 * 1024,
            maximum_packet_size: 32 * 1024,
            channel_buffer_size: event_buffer_size,
            ..Default::default()
        });

        let (event_tx, event_rx) = mpsc::channel::<SessionEvent>(event_buffer_size);
        let filtered_channels = Arc::new(Mutex::new(HashSet::new()));
        let handler = SshHandler::new(
            options.hostname.clone(),
            options.port,
            options.logical_host_id,
            Arc::clone(&options.host_key),
            event_tx,
            options.generation,
            Arc::clone(&filtered_channels),
        );

        let mut handle = if let Some(proxy) = options.proxy.as_ref() {
            let stream = spawn_proxy_stream(proxy).await?;
            tokio::time::timeout(
                options.connect_timeout,
                client::connect_stream(config, stream, handler),
            )
            .await
            .map_err(|_| SshError::Timeout)??
        } else {
            tokio::time::timeout(
                options.connect_timeout,
                client::connect(config, (&options.hostname[..], options.port), handler),
            )
            .await
            .map_err(|_| SshError::Timeout)??
        };

        tokio::time::timeout(options.connect_timeout, authenticate(&mut handle, &options))
            .await
            .map_err(|_| SshError::Timeout)??;

        Ok((
            SshConnection {
                handle,
                generation: options.generation,
                filtered_channels,
                _jump: None,
            },
            event_rx,
        ))
    }

    /// Chains the connection through a bastion: SSH to the jump host,
    /// open a direct-tcpip channel to the target ssh port, then run a
    /// second SSH session over that channel.
    async fn connect_via_jump(
        options: ConnectionOptions,
        jump: JumpSpec,
    ) -> Result<(SshConnection, mpsc::Receiver<SessionEvent>), SshError> {
        if jump.hostname.trim().is_empty() || jump.port == 0 || jump.username.trim().is_empty() {
            return Err(SshError::InvalidConfiguration(
                "jump host requires hostname, port and username".into(),
            ));
        }
        if jump.auth.is_empty() {
            return Err(SshError::InvalidConfiguration(
                "jump host requires at least one auth method".into(),
            ));
        }

        // 1. SSH to the bastion (own host-key check, own auth).
        let mut jump_options = ConnectionOptions::new(
            jump.hostname.clone(),
            jump.port,
            jump.username.clone(),
            jump.auth.clone(),
            Arc::clone(&options.host_key),
            options.generation,
        );
        jump_options.connect_timeout = options.connect_timeout;
        let (jump_connection, mut jump_events) = Self::connect_direct(jump_options).await?;

        // The bastion transport has no terminal traffic; drain its event
        // stream so the bounded channel never blocks the bastion handler.
        tokio::spawn(async move { while let Some(_event) = jump_events.recv().await {} });

        // 2. Direct-tcpip channel to the target ssh port.
        let channel = jump_connection
            .forward_channel(&options.hostname, u32::from(options.port))
            .await?;
        let stream = channel.into_stream();

        // 3. Second SSH session over the tunnel.
        let config = Arc::new(client::Config {
            inactivity_timeout: options.inactivity_timeout,
            keepalive_interval: options.keepalive_interval,
            window_size: 8 * 1024 * 1024,
            maximum_packet_size: 32 * 1024,
            channel_buffer_size: options.event_buffer_size.max(8),
            ..Default::default()
        });
        let (event_tx, event_rx) = mpsc::channel::<SessionEvent>(options.event_buffer_size.max(8));
        let filtered_channels = Arc::new(Mutex::new(HashSet::new()));
        let handler = SshHandler::new(
            options.hostname.clone(),
            options.port,
            options.logical_host_id,
            Arc::clone(&options.host_key),
            event_tx,
            options.generation,
            Arc::clone(&filtered_channels),
        );
        let mut handle = tokio::time::timeout(
            options.connect_timeout,
            client::connect_stream(config, stream, handler),
        )
        .await
        .map_err(|_| SshError::Timeout)??;
        tokio::time::timeout(options.connect_timeout, authenticate(&mut handle, &options))
            .await
            .map_err(|_| SshError::Timeout)??;

        Ok((
            SshConnection {
                handle,
                generation: options.generation,
                filtered_channels,
                _jump: Some(Box::new(jump_connection)),
            },
            event_rx,
        ))
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Opens an interactive PTY shell channel (xterm-256color).
    pub async fn open_pty(&self, cols: u32, rows: u32) -> Result<SshPty, SshError> {
        let channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;
        let (read, write) = channel.split();
        // russh mirrors every packet into the channel buffer before calling
        // the handler; an unconsumed buffer stalls the driver on large
        // outputs. Dropping the read half keeps the handler.data() callback
        // as the only data path.
        drop(read);
        Ok(SshPty { channel: write })
    }

    /// Runs one command over a non-PTY exec channel. `want_pty` is useful
    /// for programs that require a tty (e.g. `herdr status`).
    pub async fn exec(
        &self,
        command: &str,
        want_pty: bool,
        cols: u32,
        rows: u32,
    ) -> Result<SshExec, SshError> {
        let channel = self.handle.channel_open_session().await?;
        if want_pty {
            channel
                .request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
                .await?;
        }
        channel.exec(true, command.as_bytes().to_vec()).await?;
        let (read, write) = channel.split();
        drop(read);
        Ok(SshExec { channel: write })
    }

    /// Opens a direct-tcpip channel to `remote_host:remote_port` and
    /// registers it so its traffic never reaches the terminal event
    /// stream. The caller converts it into a stream for local port
    /// forwarding / Web Preview.
    pub async fn forward_channel(
        &self,
        remote_host: &str,
        remote_port: u32,
    ) -> Result<Channel<russh::client::Msg>, SshError> {
        let channel = self
            .handle
            .channel_open_direct_tcpip(remote_host, remote_port, "127.0.0.1", 0)
            .await
            .map_err(SshError::from)?;
        let channel_id = channel.id().number();
        self.filtered_channels
            .lock()
            .map_err(|_| SshError::Cancelled)?
            .insert(channel_id);
        Ok(channel)
    }

    /// Removes a forwarding/exec/subsystem channel from the terminal-event
    /// filter after its owner has finished.  Forwarding channels outlive this
    /// method, so their caller is responsible for invoking this exactly once
    /// when the stream is closed.
    pub fn release_filtered_channel(&self, channel_id: u32) {
        if let Ok(mut guard) = self.filtered_channels.lock() {
            guard.remove(&channel_id);
        }
    }

    /// Opens an SFTP subsystem session over this transport. SFTP
    /// channel traffic is excluded from the terminal event stream.
    pub async fn sftp(&self) -> Result<Arc<russh_sftp::client::SftpSession>, SshError> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let channel_id = channel.id().number();
        self.filtered_channels
            .lock()
            .map_err(|_| SshError::Cancelled)?
            .insert(channel_id);
        let stream = channel.into_stream();
        let session = russh_sftp::client::SftpSession::new_with_config(
            stream,
            russh_sftp::client::Config {
                max_packet_len: 256 * 1024,
                max_concurrent_writes: 16,
                request_timeout_secs: 10,
            },
        )
        .await
        .map_err(|error| {
            self.release_filtered_channel(channel_id);
            SshError::Protocol(format!("sftp init failed: {error}"))
        })?;
        session.set_timeout(10);
        Ok(Arc::new(session))
    }

    /// Runs one command to completion and captures its output (bounded).
    ///
    /// The exec channel is drained via `wait()` so the russh channel
    /// buffer never stalls; stdout/stderr are capped to guard against
    /// runaway output.
    pub async fn run_command(
        &self,
        command: &str,
        timeout: Duration,
        max_capture: usize,
    ) -> Result<CommandOutput, SshError> {
        // Bound channel creation and exec acknowledgement too. Previously the
        // deadline started only after both awaited successfully, so a busy
        // transport could leave runtime detection stuck forever.
        let outcome = tokio::time::timeout(timeout, async {
            let mut channel = self.handle.channel_open_session().await?;
            let channel_id = channel.id().number();
            self.filtered_channels
                .lock()
                .map_err(|_| SshError::Cancelled)?
                .insert(channel_id);
            struct FilterGuard<'a> {
                connection: &'a SshConnection,
                channel_id: u32,
            }
            impl Drop for FilterGuard<'_> {
                fn drop(&mut self) {
                    self.connection.release_filtered_channel(self.channel_id);
                }
            }
            let _filter = FilterGuard {
                connection: self,
                channel_id,
            };
            channel.exec(true, command.as_bytes().to_vec()).await?;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut stdout_truncated = false;
            let mut stderr_truncated = false;
            let mut exit_code = None;
            let mut saw_eof = false;
            while let Some(message) = channel.wait().await {
                match message {
                    russh::ChannelMsg::Data { data } => {
                        if stdout.len() < max_capture {
                            stdout.extend_from_slice(&data);
                        } else {
                            stdout_truncated = true;
                        }
                    }
                    russh::ChannelMsg::ExtendedData { data, ext: 1 } => {
                        if stderr.len() < max_capture {
                            stderr.extend_from_slice(&data);
                        } else {
                            stderr_truncated = true;
                        }
                    }
                    russh::ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status);
                        if saw_eof {
                            break;
                        }
                    }
                    // RFC 4254 permits exit-status after EOF. Breaking on
                    // EOF loses a valid status and turns successful remote
                    // probes into the synthetic code -1.
                    russh::ChannelMsg::Eof => {
                        saw_eof = true;
                        if exit_code.is_some() {
                            break;
                        }
                    }
                    russh::ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok::<CommandOutput, SshError>(CommandOutput {
                stdout,
                stderr,
                exit_code: exit_code.map(|code| i32::try_from(code).unwrap_or(-1)),
                stdout_truncated,
                stderr_truncated,
            })
        })
        .await;
        outcome.map_err(|_| SshError::Timeout)?
    }

    /// Gracefully closes the transport. Remote sessions survive.
    pub async fn disconnect(&self) -> Result<(), SshError> {
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "client closing", "en")
            .await
            .map_err(SshError::from)
    }
}

struct ProxyStream {
    child: Child,
    reader: ChildStdout,
    writer: ChildStdin,
}

impl Drop for ProxyStream {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl AsyncRead for ProxyStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(context, buffer)
    }
}

impl AsyncWrite for ProxyStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(context, buffer)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(context)
    }
}

async fn spawn_proxy_stream(proxy: &ProxyCommand) -> Result<ProxyStream, SshError> {
    let mut command = Command::new(&proxy.program);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command
        .args(&proxy.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| SshError::Io(error.to_string()))?;
    let reader = child
        .stdout
        .take()
        .ok_or_else(|| SshError::Io("proxy stdout pipe unavailable".into()))?;
    let writer = child
        .stdin
        .take()
        .ok_or_else(|| SshError::Io("proxy stdin pipe unavailable".into()))?;
    Ok(ProxyStream {
        child,
        reader,
        writer,
    })
}

/// An open PTY shell channel bound to one connection.
pub struct SshPty {
    channel: russh::ChannelWriteHalf<russh::client::Msg>,
}

impl SshPty {
    /// The SSH channel number carrying this PTY (used to route
    /// session events to the right split pane).
    #[must_use]
    pub fn channel_id(&self) -> u32 {
        self.channel.id().number()
    }

    pub async fn write(&self, data: &[u8]) -> Result<(), SshError> {
        self.channel
            .data_bytes(data.to_vec())
            .await
            .map_err(SshError::from)
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.channel
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(SshError::from)
    }

    pub async fn eof(&self) -> Result<(), SshError> {
        self.channel.eof().await.map_err(SshError::from)
    }

    pub async fn close(&self) -> Result<(), SshError> {
        self.channel.close().await.map_err(SshError::from)
    }
}

/// An open exec channel.
pub struct SshExec {
    channel: russh::ChannelWriteHalf<russh::client::Msg>,
}

impl SshExec {
    pub async fn close(&self) -> Result<(), SshError> {
        self.channel.close().await.map_err(SshError::from)
    }
}

async fn authenticate(
    handle: &mut client::Handle<SshHandler>,
    options: &ConnectionOptions,
) -> Result<(), SshError> {
    for method in &options.auth {
        match method {
            AuthMethod::Password(password) => {
                // The lossy copy is a plain heap string; zero it as soon
                // as authentication returns so the secret does not linger.
                let mut password = String::from_utf8_lossy(password.as_slice()).into_owned();
                let result = handle
                    .authenticate_password(&options.username, &password)
                    .await?;
                password.zeroize();
                if result.success() {
                    return Ok(());
                }
            }
            AuthMethod::PublicKey {
                key_path,
                passphrase,
            } => {
                let mut passphrase_str = passphrase
                    .as_ref()
                    .map(|value| String::from_utf8_lossy(value.as_slice()).into_owned());
                let key = load_secret_key(key_path, passphrase_str.as_deref())?;
                if let Some(value) = passphrase_str.as_mut() {
                    value.zeroize();
                }
                let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
                let result = handle
                    .authenticate_publickey(
                        &options.username,
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                    )
                    .await?;
                if result.success() {
                    return Ok(());
                }
            }
            AuthMethod::SshAgent => {
                if authenticate_with_windows_agent(handle, &options.username).await? {
                    return Ok(());
                }
            }
            AuthMethod::KeyboardInteractive { broker } => {
                use russh::client::KeyboardInteractiveAuthResponse;
                let mut reply = handle
                    .authenticate_keyboard_interactive_start(&options.username, None::<String>)
                    .await?;
                loop {
                    match reply {
                        KeyboardInteractiveAuthResponse::Success => return Ok(()),
                        KeyboardInteractiveAuthResponse::Failure { .. } => break,
                        KeyboardInteractiveAuthResponse::InfoRequest {
                            name,
                            instructions,
                            prompts,
                        } => {
                            let expected = prompts.len();
                            let responses = broker
                                .prompt(
                                    name,
                                    instructions,
                                    prompts
                                        .into_iter()
                                        .map(|prompt| crate::keyboard_interactive::KeyboardPrompt {
                                            prompt: prompt.prompt,
                                            echo: prompt.echo,
                                        })
                                        .collect(),
                                )
                                .await?;
                            if responses.len() != expected {
                                return Err(SshError::InvalidConfiguration(
                                    "keyboard-interactive response count mismatch".into(),
                                ));
                            }
                            reply = handle
                                .authenticate_keyboard_interactive_respond(responses)
                                .await?;
                        }
                    }
                }
            }
        }
    }
    Err(SshError::AuthenticationFailed)
}

async fn authenticate_from_agent<S>(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
    mut agent: AgentClient<S>,
) -> Result<bool, SshError>
where
    S: russh::keys::agent::client::AgentStream + Send + Unpin,
{
    let identities = agent.request_identities().await.map_err(|error| {
        SshError::Protocol(format!("SSH agent identity request failed: {error}"))
    })?;
    if identities.is_empty() {
        return Err(SshError::InvalidConfiguration(
            "SSH agent contains no identities".into(),
        ));
    }
    for identity in identities {
        let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
        let result = match identity {
            AgentIdentity::PublicKey { key, .. } => handle
                .authenticate_publickey_with(username, key, hash_alg, &mut agent)
                .await
                .map_err(|error| {
                    SshError::Protocol(format!("SSH agent signing failed: {error}"))
                })?,
            AgentIdentity::Certificate { certificate, .. } => handle
                .authenticate_certificate_with(username, certificate, hash_alg, &mut agent)
                .await
                .map_err(|error| {
                    SshError::Protocol(format!("SSH agent signing failed: {error}"))
                })?,
        };
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
async fn authenticate_with_windows_agent(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
) -> Result<bool, SshError> {
    const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";
    match AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE).await {
        Ok(agent) => return authenticate_from_agent(handle, username, agent).await,
        Err(open_ssh_error) => match AgentClient::connect_pageant().await {
            Ok(agent) => authenticate_from_agent(handle, username, agent).await,
            Err(pageant_error) => Err(SshError::InvalidConfiguration(format!(
                "no usable Windows SSH agent (OpenSSH: {open_ssh_error}; Pageant: {pageant_error})"
            ))),
        },
    }
}

#[cfg(not(windows))]
async fn authenticate_with_windows_agent(
    _handle: &mut client::Handle<SshHandler>,
    _username: &str,
) -> Result<bool, SshError> {
    Err(SshError::AuthMethodUnavailable(
        "Windows SSH agent authentication is only available on Windows",
    ))
}
/// Bounded output of a completed remote command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}
