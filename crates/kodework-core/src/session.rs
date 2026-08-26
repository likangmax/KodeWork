#![forbid(unsafe_code)]

//! Connection/session manager: address fallback, generation-guarded event
//! streams, pane routing, and PTY control. Connection state and transport
//! generations are guarded by one authoritative controller; reconnect policy
//! may live above this crate, but it can no longer bypass lifecycle rules.

use crate::tunnel::{TunnelInfo, TunnelManager};
use kodework_domain::{
    validate_remote_path, Action, ActionMode, BridgeId, ConnectionState, Host, HostId, RunId,
    TransferDirection, TransferId,
};
use kodework_herdr::cli::{ExecOutput, HerdrClient, RemoteExecutor};
use kodework_herdr::HerdrError;
use kodework_network::CandidateResolver;
use kodework_sftp::backend::{RemoteFileMeta, RusshSftpBackend, SftpBackend};
use kodework_sftp::manager::{TransferEvent, TransferLeaseRegistry, TransferManager};
use kodework_sftp::{TransferRequest, DEFAULT_MAX_CONCURRENCY};
use kodework_ssh::connection::{
    AuthMethod, CommandExecutionError, CommandOutput, ConnectionOptions, JumpSpec, ProxyCommand,
    SshConnection, SshPty,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConnectErrorKind {
    Network,
    Timeout,
    Tailscale,
    Authentication,
    CredentialRequired,
    HostKey,
    InvalidConfiguration,
    Cancelled,
    Protocol,
    Internal,
}

impl ConnectErrorKind {
    #[must_use]
    pub fn retryable(self) -> bool {
        matches!(self, Self::Network | Self::Timeout | Self::Tailscale)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectError {
    pub kind: ConnectErrorKind,
    pub detail: String,
}

impl ConnectError {
    fn internal(detail: impl Into<String>) -> Self {
        Self {
            kind: ConnectErrorKind::Internal,
            detail: detail.into(),
        }
    }

    fn invalid(detail: impl Into<String>) -> Self {
        Self {
            kind: ConnectErrorKind::InvalidConfiguration,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.detail)
    }
}

impl std::fmt::Display for ConnectErrorKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Tailscale => "tailscale",
            Self::Authentication => "authentication",
            Self::CredentialRequired => "credential_required",
            Self::HostKey => "host_key",
            Self::InvalidConfiguration => "invalid_configuration",
            Self::Cancelled => "cancelled",
            Self::Protocol => "protocol",
            Self::Internal => "internal",
        })
    }
}

impl From<kodework_ssh::SshError> for ConnectError {
    fn from(error: kodework_ssh::SshError) -> Self {
        use kodework_ssh::SshError;
        let kind = match &error {
            SshError::Timeout => ConnectErrorKind::Timeout,
            SshError::ConnectionRefused | SshError::Unreachable | SshError::NameResolution(_) => {
                ConnectErrorKind::Network
            }
            SshError::AuthenticationFailed => ConnectErrorKind::Authentication,
            SshError::CredentialRequired(_) => ConnectErrorKind::CredentialRequired,
            SshError::HostKeyChanged
            | SshError::HostKeyRejected
            | SshError::HostKeyDecisionTimeout
            | SshError::HostKeyStoreUnavailable(_) => ConnectErrorKind::HostKey,
            SshError::Cancelled => ConnectErrorKind::Cancelled,
            SshError::InvalidConfiguration(_) | SshError::AuthMethodUnavailable(_) => {
                ConnectErrorKind::InvalidConfiguration
            }
            SshError::Protocol(_) | SshError::MissingExitStatus | SshError::ChannelClosed => {
                ConnectErrorKind::Protocol
            }
            SshError::Io(_) => ConnectErrorKind::Internal,
        };
        Self {
            kind,
            detail: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionRuntimeSnapshot {
    pub state: ConnectionState,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransitionError {
    Invalid {
        from: ConnectionState,
        to: ConnectionState,
    },
    StaleGeneration {
        expected: u64,
        actual: u64,
    },
}

impl std::fmt::Display for StateTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { from, to } => {
                write!(
                    formatter,
                    "invalid connection transition: {from:?} -> {to:?}"
                )
            }
            Self::StaleGeneration { expected, actual } => write!(
                formatter,
                "stale connection generation: expected {expected}, current {actual}"
            ),
        }
    }
}

impl std::error::Error for StateTransitionError {}

/// The sole mutable authority for a session's lifecycle state and transport
/// generation. Callers cannot write either field directly, which prevents a
/// stale event pump from overwriting a newer connection.
pub struct ConnectionStateController {
    state: Mutex<ConnectionState>,
    generation: AtomicU64,
}

impl Default for ConnectionStateController {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionStateController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ConnectionState::Disconnected),
            generation: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> ConnectionRuntimeSnapshot {
        ConnectionRuntimeSnapshot {
            state: self
                .state
                .lock()
                .map(|value| *value)
                .unwrap_or(ConnectionState::Disconnected),
            generation: self.generation.load(Ordering::SeqCst),
        }
    }

    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.snapshot().state
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn transition(&self, to: ConnectionState) -> Result<(), StateTransitionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StateTransitionError::Invalid {
                from: ConnectionState::Failed,
                to,
            })?;
        if *state != to && !kodework_domain::connection_transition(*state, to) {
            return Err(StateTransitionError::Invalid { from: *state, to });
        }
        *state = to;
        Ok(())
    }

    pub fn transition_for_generation(
        &self,
        generation: u64,
        to: ConnectionState,
    ) -> Result<(), StateTransitionError> {
        let actual = self.generation();
        if actual != generation {
            return Err(StateTransitionError::StaleGeneration {
                expected: generation,
                actual,
            });
        }
        self.transition(to)
    }

    /// Reserve the next transport generation. The state itself is unchanged;
    /// the connect path transitions it explicitly after address resolution.
    pub fn reserve_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Installs a generation obtained by the connect path. This is monotonic
    /// and rejects stale transport attachment.
    pub fn install_generation(&self, generation: u64) -> Result<(), StateTransitionError> {
        let current = self.generation();
        if generation < current {
            return Err(StateTransitionError::StaleGeneration {
                expected: generation,
                actual: current,
            });
        }
        self.generation.store(generation, Ordering::SeqCst);
        Ok(())
    }
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
    controller: Arc<ConnectionStateController>,
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
    herdr_bridges: Arc<Mutex<HashMap<BridgeId, ActiveBridge>>>,
}

struct ActiveBridge {
    owner: Arc<kodework_ssh::connection::SshExec>,
    tunnel_id: crate::tunnel::TunnelInfo,
    remote_port: u16,
    generation: u64,
}

impl Clone for ActiveSession {
    fn clone(&self) -> Self {
        Self {
            controller: Arc::clone(&self.controller),
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
        self.connect_with_jump_auth_typed(host, auth, None)
            .await
            .map_err(|error| error.to_string())
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
        self.connect_with_jump_auth_typed(host, auth, jump_auth)
            .await
            .map_err(|error| error.to_string())
    }

    /// Typed connection boundary used by native supervision. The compatibility
    /// methods above stringify only at their public legacy boundary; retry and
    /// credential policy never inspect those strings.
    pub async fn connect_with_jump_auth_typed(
        &self,
        host: &Host,
        auth: Vec<AuthMethod>,
        jump_auth: Option<Vec<AuthMethod>>,
    ) -> Result<SessionOutcome, ConnectError> {
        self.ensure_session(host.id);
        let connect_guard = self
            .sessions
            .lock()
            .map_err(|_| ConnectError::internal("session registry poisoned"))?
            .get(&host.id)
            .cloned()
            .ok_or_else(|| ConnectError::internal("session was not created"))?
            .connect_guard;
        let _connect_guard = connect_guard.lock().await;
        self.set_state(host.id, ConnectionState::ResolvingAddress)?;
        let candidates = self.resolver.candidates(host).await;
        if candidates.is_empty() {
            self.set_state(host.id, ConnectionState::Failed)?;
            return Err(ConnectError::invalid("no enabled address candidates"));
        }
        let generation = self.next_generation(host.id);
        let jump_auth = jump_auth.unwrap_or_else(|| auth.clone());

        let mut last_error = ConnectError::internal("no candidate attempted");
        for candidate in candidates {
            self.set_state(host.id, ConnectionState::Connecting)?;
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
                    self.attach(host.id, connection, events, generation)?;
                    self.set_state(host.id, ConnectionState::Ready)?;
                    return Ok(SessionOutcome::Connected {
                        host_id: host.id,
                        generation,
                    });
                }
                Err(error) => {
                    last_error = error.clone().into();
                    if !kodework_ssh::address_fallback_is_retryable(&error) {
                        self.set_state(host.id, ConnectionState::Failed)?;
                        return Err(ConnectError {
                            kind: last_error.kind,
                            detail: format!(
                                "candidate {}: {}",
                                candidate.address.hostname_or_ip, last_error.detail
                            ),
                        });
                    }
                    // Network-class failures continue to the next candidate.
                }
            }
        }
        self.set_state(host.id, ConnectionState::Failed)?;
        Err(ConnectError {
            kind: last_error.kind,
            detail: format!("all candidates failed: {}", last_error.detail),
        })
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
        session.controller.state()
    }

    #[must_use]
    pub fn runtime_snapshot(&self, host_id: HostId) -> ConnectionRuntimeSnapshot {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&host_id).cloned())
            .map(|session| session.controller.snapshot())
            .unwrap_or(ConnectionRuntimeSnapshot {
                state: ConnectionState::Disconnected,
                generation: 0,
            })
    }

    /// Marks a transport as waiting for fresh user input. The native
    /// supervisor uses this instead of retrying a credential failure forever.
    pub fn mark_waiting_for_credential(&self, host_id: HostId) -> Result<(), ConnectError> {
        self.set_state(host_id, ConnectionState::WaitingForCredential)
    }

    /// Keeps a transient network failure under native supervisor ownership so
    /// the next supervisor tick can make another bounded attempt. This is
    /// intentionally separate from `WaitingForCredential` and `Failed`.
    pub fn mark_reconnecting(&self, host_id: HostId) -> Result<(), ConnectError> {
        self.set_state(host_id, ConnectionState::Reconnecting)
    }

    #[must_use]
    pub fn generation(&self, host_id: HostId) -> u64 {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| {
                sessions
                    .get(&host_id)
                    .map(|session| session.controller.generation())
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
        self.run_remote_with_timeout_tracked(host_id, command, timeout)
            .await
            .map_err(|error| error.to_string())
    }

    async fn run_remote_with_timeout_tracked(
        &self,
        host_id: HostId,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, ActionRunError> {
        let connection = self
            .sessions
            .lock()
            .map_err(|_| ActionRunError::before_dispatch("session registry poisoned"))?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| ActionRunError::before_dispatch("no session for host"))?
            .connection
            .lock()
            .map_err(|_| ActionRunError::before_dispatch("connection lock poisoned"))?
            .clone()
            .ok_or_else(|| ActionRunError::before_dispatch("not connected"))?;
        connection
            .run_command_tracked(command, timeout, DEFAULT_CAPTURE_CAP)
            .await
            .map_err(ActionRunError::from_command)
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
            Arc::clone(&manager),
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
        let base_port = 28000u16 + u16::try_from(host_id.as_uuid().as_u128() % 2000).unwrap_or(0);
        let generation = self.generation(host_id);
        let occupied = session
            .herdr_bridges
            .lock()
            .map_err(|_| "bridge registry poisoned".to_string())?
            .values()
            .map(|bridge| bridge.remote_port)
            .collect::<std::collections::HashSet<_>>();
        let mut last_error = String::from("no bridge port candidate succeeded");
        for offset in 0..16u16 {
            let remote_port = 28000u16 + ((base_port - 28000 + offset) % 2000);
            if occupied.contains(&remote_port) {
                continue;
            }
            let owner = match connection
                .exec_owned(&format!(
                    "exec socat TCP-LISTEN:{remote_port},bind=127.0.0.1,reuseaddr,fork UNIX-CONNECT:{quoted_socket}"
                ))
                .await
            {
                Ok(owner) => Arc::new(owner),
                Err(error) => {
                    last_error = format!("socat port {remote_port} failed: {error}");
                    continue;
                }
            };
            if let Err(error) = owner.ensure_running(Duration::from_millis(250)).await {
                let _ = owner.close().await;
                last_error = format!("socat port {remote_port} exited during startup: {error}");
                continue;
            }
            let tunnel = match self
                .open_tunnel(host_id, local_port, "127.0.0.1", remote_port)
                .await
            {
                Ok(tunnel) => tunnel,
                Err(error) => {
                    let _ = owner.close().await;
                    last_error = error;
                    continue;
                }
            };
            let bridge_id = BridgeId::new();
            let info = HerdrBridgeInfo {
                bridge_id,
                tunnel: tunnel.clone(),
                remote_socket: socket_path.clone(),
                remote_port,
            };
            session
                .herdr_bridges
                .lock()
                .map_err(|_| "bridge registry poisoned".to_string())?
                .insert(
                    bridge_id,
                    ActiveBridge {
                        owner,
                        tunnel_id: tunnel,
                        remote_port,
                        generation,
                    },
                );
            return Ok(info);
        }
        Err(last_error)
    }

    pub async fn herdr_bridge_stop_by_id(
        &self,
        host_id: HostId,
        bridge_id: BridgeId,
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
            .remove(&bridge_id);
        if let Some(bridge) = owner {
            // A BridgeId is scoped to the transport generation.  The current
            // generation may be newer after reconnect, but stopping an old
            // id remains idempotent and must never affect a new bridge.
            let _bridge_generation = bridge.generation;
            let _ = self.tunnels.close(bridge.tunnel_id.id).await;
            bridge
                .owner
                .close()
                .await
                .map_err(|error| error.to_string())?;
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
    ) -> Result<RunOutcome, ActionRunError> {
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
    ) -> Result<RunOutcome, ActionRunError> {
        // Recompute the danger level server-side: the renderer-declared
        // field is only a hint and must never gate confirmation.
        let requires_confirmation = crate::action_requires_confirmation(action);
        if requires_confirmation && !confirmed {
            return Err(ActionRunError::before_dispatch("该动作需要确认后才能运行"));
        }
        let command = build_action_command(action).map_err(ActionRunError::before_dispatch)?;
        match action.mode {
            ActionMode::Interactive => {
                let pty = self.first_pane_pty(host_id)?;
                let mut line = command;
                line.push('\r');
                pty.write(line.as_bytes())
                    .await
                    .map_err(|error| ActionRunError::before_dispatch(error.to_string()))?;
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
                    .run_remote_with_timeout_tracked(host_id, &run_script, timeout)
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
                    .run_remote_with_timeout_tracked(
                        host_id,
                        &tmux_command,
                        DEFAULT_COMMAND_TIMEOUT,
                    )
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
        run_id: RunId,
        mode: ActionMode,
    ) -> Result<RemoteRunState, String> {
        let request = RemoteRunProbeRequest { run_id, mode };
        self.reconcile_remote_runs(host_id, std::slice::from_ref(&request))
            .await?
            .into_iter()
            .next()
            .map(|probe| probe.state)
            .ok_or_else(|| "remote run probe returned no result".to_string())
    }

    /// Probes a bounded batch of persisted runs in one SSH exec.  Run ids are
    /// UUID hex strings validated before interpolation, and the mode controls
    /// the only authoritative live signal: Background requires its owned tmux
    /// session; Quick never treats a started marker as proof of liveness.
    pub async fn reconcile_remote_runs(
        &self,
        host_id: HostId,
        requests: &[RemoteRunProbeRequest],
    ) -> Result<Vec<RemoteRunProbe>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let command = build_remote_run_probe_command(requests)?;
        let output = self.run_remote(host_id, &command).await?;
        let mut parsed = std::collections::HashMap::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split('\t');
            let Some(id) = fields.next() else { continue };
            let Some(request) = requests
                .iter()
                .find(|request| request.run_id.as_uuid().simple().to_string() == id)
            else {
                continue;
            };
            let state = match fields.next() {
                Some("completed") => fields
                    .next()
                    .and_then(|value| value.parse::<i32>().ok())
                    .map(|exit_code| RemoteRunState::Completed {
                        exit_code,
                        started_at_ms: parse_epoch_seconds(fields.next()),
                        finished_at_ms: parse_epoch_seconds(fields.next()),
                    })
                    .unwrap_or(RemoteRunState::Unknown),
                Some("running") if request.mode == ActionMode::Background => {
                    RemoteRunState::Running
                }
                _ => RemoteRunState::Unknown,
            };
            parsed.insert(request.run_id, state);
        }
        Ok(requests
            .iter()
            .map(|request| RemoteRunProbe {
                run_id: request.run_id,
                state: parsed
                    .get(&request.run_id)
                    .copied()
                    .unwrap_or(RemoteRunState::Unknown),
            })
            .collect())
    }

    /// Best-effort cleanup after local durable persistence.  A cleanup error
    /// is intentionally ignored by callers: local run history is authoritative
    /// and must not be rolled back because remote metadata is unavailable.
    pub async fn cleanup_remote_run_metadata(
        &self,
        host_id: HostId,
        run_id: RunId,
    ) -> Result<(), String> {
        let id = run_id.as_uuid().simple().to_string();
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("remote run id is not a safe UUID token".to_string());
        }
        let command = format!("base=\"$HOME/.cache/kodework/runs/{id}\"; rm -rf -- \"$base\"");
        self.run_remote(host_id, &command).await.map(|_| ())
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
        if let Some(slot) = session
            .transfers
            .lock()
            .map_err(|_| "transfers lock poisoned".to_string())?
            .take()
        {
            slot.manager.event_pump_stopped();
        }
        let bridges = session
            .herdr_bridges
            .lock()
            .map_err(|_| "bridge registry poisoned".to_string())?
            .drain()
            .map(|(_, bridge)| bridge.owner)
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
        self.set_state(host_id, ConnectionState::Disconnected)
            .map_err(|error| error.to_string())?;
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
            .map(|session| session.controller.reserve_generation())
            .unwrap_or(1)
    }

    fn ensure_session(&self, host_id: HostId) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions.entry(host_id).or_insert_with(|| ActiveSession {
            controller: Arc::new(ConnectionStateController::new()),
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
    ) -> Result<(), ConnectError> {
        let session = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&host_id)
            .cloned()
            .unwrap_or_else(|| unreachable!("session must exist"));
        session
            .controller
            .install_generation(generation)
            .map_err(|error| ConnectError::internal(error.to_string()))?;
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
            .map(|(_, bridge)| bridge.owner)
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
        let session_for_pump = Arc::new(session);
        tokio::spawn(async move {
            pump_events(session_for_pump, events, generation).await;
        });
        Ok(())
    }

    fn set_state(&self, host_id: HostId, state: ConnectionState) -> Result<(), ConnectError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| ConnectError::internal("session registry poisoned"))?
            .get(&host_id)
            .cloned()
            .ok_or_else(|| ConnectError::internal("session was not created"))?;
        session
            .controller
            .transition(state)
            .map_err(|error| ConnectError::internal(error.to_string()))
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
        if session.controller.generation() != generation {
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
                if session.controller.generation() != generation {
                    continue;
                }
                if session.controller.state() == ConnectionState::Ready {
                    let _ = session
                        .controller
                        .transition_for_generation(generation, ConnectionState::Reconnecting);
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
/// counter. Always running while the manager is alive, so the manager never
/// blocks on its event buffer. When this pump exits, it marks the manager
/// dead: in-flight workers then abort at their next chunk boundary instead
/// of blocking on a full channel while nobody drains it.
async fn pump_transfer_events(
    mut events: mpsc::Receiver<TransferEvent>,
    subscribers: Arc<Mutex<Vec<mpsc::Sender<TransferEvent>>>>,
    dropped_events: Arc<AtomicU64>,
    manager: Arc<TransferManager>,
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
    // Nothing drains the manager's event stream any more. Tell in-flight
    // workers to stop cleanly rather than blocking on the channel forever.
    manager.event_pump_stopped();
}
/// Result of bridging the remote herdr socket to a local port.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HerdrBridgeInfo {
    pub bridge_id: BridgeId,
    pub tunnel: crate::tunnel::TunnelInfo,
    pub remote_socket: String,
    pub remote_port: u16,
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

/// Action execution failure with the protocol fact needed to classify the
/// persisted Run. `dispatched` means the exec request may have reached the
/// server and was not explicitly rejected; a later failure is therefore
/// Unknown, not proof that the remote command failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRunError {
    message: String,
    dispatched: bool,
    timed_out: bool,
}

impl ActionRunError {
    fn before_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            dispatched: false,
            timed_out: false,
        }
    }

    fn from_command(error: CommandExecutionError) -> Self {
        let timed_out = matches!(error.source, kodework_ssh::SshError::Timeout);
        Self {
            message: error.to_string(),
            dispatched: error.dispatched,
            timed_out,
        }
    }

    #[must_use]
    pub fn was_dispatched(&self) -> bool {
        self.dispatched
    }

    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.timed_out
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<String> for ActionRunError {
    fn from(message: String) -> Self {
        Self::before_dispatch(message)
    }
}

impl std::fmt::Display for ActionRunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ActionRunError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteRunProbeRequest {
    pub run_id: RunId,
    pub mode: ActionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteRunProbe {
    pub run_id: RunId,
    pub state: RemoteRunState,
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

fn build_remote_run_probe_command(requests: &[RemoteRunProbeRequest]) -> Result<String, String> {
    let mut command = String::from("set -f; ");
    for request in requests {
        let id = request.run_id.as_uuid().simple().to_string();
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("remote run id is not a safe UUID token".to_string());
        }
        let mode = match request.mode {
            ActionMode::Background => "background",
            ActionMode::Quick | ActionMode::Interactive => "quick",
        };
        command.push_str(&format!(
            "id='{id}'; base=\"$HOME/.cache/kodework/runs/$id\"; if [ -f \"$base/exit_code\" ]; then printf '%s\\tcompleted\\t%s\\t%s\\t%s\\n' \"$id\" \"$(cat -- \"$base/exit_code\" 2>/dev/null || true)\" \"$(cat -- \"$base/started_at_s\" 2>/dev/null || true)\" \"$(cat -- \"$base/finished_at_s\" 2>/dev/null || true)\"; elif [ '{mode}' = 'background' ] && tmux has-session -t \"kodework-run-$id\" >/dev/null 2>&1; then printf '%s\\trunning\\n' \"$id\"; else printf '%s\\tunknown\\n' \"$id\"; fi; "
        ));
    }
    Ok(command)
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

    #[test]
    fn remote_run_probe_uses_mode_specific_live_evidence() {
        let quick = RunId::new();
        let background = RunId::new();
        let script = build_remote_run_probe_command(&[
            RemoteRunProbeRequest {
                run_id: quick,
                mode: ActionMode::Quick,
            },
            RemoteRunProbeRequest {
                run_id: background,
                mode: ActionMode::Background,
            },
        ])
        .unwrap_or_else(|error| unreachable!("safe UUIDs must build: {error}"));
        assert!(script.contains("[ 'quick' = 'background' ]"));
        assert!(script.contains("tmux has-session"));
        assert!(!script.contains("[ -f \"$base/started_at_s\" ] ||"));
        assert!(script.contains("else printf '%s\\tunknown\\n'"));
    }

    #[tokio::test]
    async fn stale_generation_events_are_dropped_before_replay() {
        let (event_tx, event_rx) = mpsc::channel(2);
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(2);
        let controller = Arc::new(ConnectionStateController::new());
        controller
            .install_generation(2)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition(ConnectionState::ResolvingAddress)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition(ConnectionState::Connecting)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition(ConnectionState::VerifyingHostKey)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition(ConnectionState::Authenticating)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition(ConnectionState::Ready)
            .unwrap_or_else(|_| unreachable!());
        let session = Arc::new(ActiveSession {
            controller,
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

    #[test]
    fn controller_rejects_invalid_and_stale_transitions() {
        let controller = ConnectionStateController::new();
        assert!(matches!(
            controller.transition(ConnectionState::Ready),
            Err(StateTransitionError::Invalid {
                from: ConnectionState::Disconnected,
                to: ConnectionState::Ready
            })
        ));
        let generation = controller.reserve_generation();
        assert_eq!(generation, 1);
        assert!(matches!(
            controller.transition_for_generation(0, ConnectionState::ResolvingAddress),
            Err(StateTransitionError::StaleGeneration {
                expected: 0,
                actual: 1
            })
        ));
        controller
            .transition_for_generation(generation, ConnectionState::ResolvingAddress)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition_for_generation(generation, ConnectionState::Connecting)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition_for_generation(generation, ConnectionState::VerifyingHostKey)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition_for_generation(generation, ConnectionState::Authenticating)
            .unwrap_or_else(|_| unreachable!());
        controller
            .transition_for_generation(generation, ConnectionState::Ready)
            .unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            controller.transition_for_generation(0, ConnectionState::Reconnecting),
            Err(StateTransitionError::StaleGeneration { .. })
        ));
        assert_eq!(controller.state(), ConnectionState::Ready);
    }

    #[test]
    fn current_transport_loss_moves_to_reconnecting_only_once() {
        let controller = ConnectionStateController::new();
        let generation = controller.reserve_generation();
        for state in [
            ConnectionState::ResolvingAddress,
            ConnectionState::Connecting,
            ConnectionState::VerifyingHostKey,
            ConnectionState::Authenticating,
            ConnectionState::Ready,
        ] {
            controller
                .transition_for_generation(generation, state)
                .unwrap_or_else(|_| unreachable!());
        }
        controller
            .transition_for_generation(generation, ConnectionState::Reconnecting)
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(controller.state(), ConnectionState::Reconnecting);
        assert!(matches!(
            controller.transition_for_generation(generation - 1, ConnectionState::Ready),
            Err(StateTransitionError::StaleGeneration { .. })
        ));
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
