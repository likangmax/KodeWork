#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord,
        )]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(HostId);
id_type!(AddressId);
id_type!(ProjectId);
id_type!(ActionId);
id_type!(RunId);
id_type!(SessionId);
id_type!(TransferId);
id_type!(TunnelId);
id_type!(SnippetId);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialRef {
    pub provider: CredentialProvider,
    pub opaque_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialProvider {
    WindowsCredentialManager,
    DpapiFile,
    /// Native OS keyring on macOS/Linux (Keychain or Secret Service).
    ///
    /// Windows installations keep using `WindowsCredentialManager` for
    /// backwards-compatible Credential Manager records.  The provider is
    /// explicit so a database imported between operating systems never
    /// silently assumes that a Windows credential is readable elsewhere.
    NativeKeyring,
    Test,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AuthenticationMode {
    #[default]
    Password,
    PublicKey,
    SshAgent,
    KeyboardInteractive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TailscaleMode {
    Disabled,
    SystemDaemon,
    EmbeddedUserspace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TailscaleConfig {
    pub enabled: bool,
    pub mode: TailscaleMode,
    pub device_name: Option<String>,
    pub auth_key_ref: Option<CredentialRef>,
    pub state_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AddressKind {
    Lan,
    Tailscale,
    Public,
    JumpHost,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address {
    pub id: AddressId,
    pub kind: AddressKind,
    pub hostname_or_ip: String,
    pub port: u16,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JumpHost {
    pub hostname: String,
    pub port: u16,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub id: HostId,
    pub label: String,
    pub username: String,
    pub port: u16,
    pub auth_ref: Option<CredentialRef>,
    /// Explicit authentication policy. Credential material itself is never
    /// stored here; `auth_ref` remains an opaque secret-store reference.
    #[serde(default)]
    pub auth_mode: AuthenticationMode,
    /// User-selected OpenSSH private-key path. This is metadata, not key
    /// material. Encrypted-key passphrases remain one-shot/secret-store data.
    #[serde(default)]
    pub private_key_path: Option<String>,
    /// Directory opened by the SFTP file panel when this Host is selected.
    #[serde(default = "default_remote_path")]
    pub default_remote_path: String,
    /// Optional bastion host: the real SSH connection is chained through it.
    #[serde(default)]
    pub jump: Option<JumpHost>,
    pub addresses: Vec<Address>,
    #[serde(default)]
    pub tailscale: Option<TailscaleConfig>,
    #[serde(default)]
    pub default_runtime: RuntimeKind,
}

fn default_remote_path() -> String {
    "/".into()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RuntimeKind {
    #[default]
    Tmux,
    Herdr,
    PlainShell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub host_id: HostId,
    pub name: String,
    pub remote_cwd: String,
    pub preferred_runtime: RuntimeKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionMode {
    Interactive,
    Quick,
    Background,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum DangerLevel {
    Safe,
    Review,
    Dangerous,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfirmationPolicy {
    Never,
    OnDangerous,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    pub project_id: ProjectId,
    pub name: String,
    pub command: String,
    pub mode: ActionMode,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    pub danger_level: DangerLevel,
    pub confirmation: ConfirmationPolicy,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Created,
    Confirming,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    /// The local application stopped before it could observe a terminal
    /// result (for example, a crash or forced process termination).
    Interrupted,
    /// The run was previously active, but neither a live remote session nor
    /// authoritative completion metadata can currently be found.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    pub id: RunId,
    /// The mutable source Action may be deleted after this historical record
    /// is created, so it is intentionally optional.
    pub action_id: Option<ActionId>,
    pub host_id: HostId,
    pub project_id: Option<ProjectId>,
    pub action_name: String,
    pub command_snapshot: String,
    pub mode: ActionMode,
    pub cwd_snapshot: Option<String>,
    pub status: RunStatus,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub remote_session_ref: Option<String>,
    pub stdout_preview: String,
    pub stderr_preview: String,
    pub output_bytes: u64,
    pub last_reconciled_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    ResolvingAddress,
    Connecting,
    VerifyingHostKey,
    Authenticating,
    Ready,
    Reconnecting,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Detached,
    Attaching,
    Attached,
    Suspended,
    Reattaching,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferStatus {
    Queued,
    Hashing,
    Transferring,
    Paused,
    Retrying,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub host_id: HostId,
    pub project_id: Option<ProjectId>,
    pub runtime: RuntimeKind,
    pub external_ref: Option<String>,
    pub state: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: TransferId,
    pub host_id: HostId,
    pub local_path: String,
    pub remote_path: String,
    pub direction: TransferDirection,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tunnel {
    pub id: TunnelId,
    pub host_id: HostId,
    pub remote_host: String,
    pub remote_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    pub id: SnippetId,
    pub name: String,
    pub text: String,
    #[serde(default)]
    pub sort_order: i32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("port must be between 1 and 65535")]
    InvalidPort,
    #[error("address contains shell control characters")]
    UnsafeAddress,
    #[error(
        "remote path must be ~, absolute, or start with ~/ and must not contain control characters"
    )]
    InvalidRemotePath,
    #[error("action command must not be empty")]
    EmptyCommand,
    #[error("invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },
}

pub fn validate_host(host: &Host) -> Result<(), DomainError> {
    if host.label.trim().is_empty() {
        return Err(DomainError::EmptyField { field: "label" });
    }
    if host.label.chars().any(char::is_control) {
        return Err(DomainError::UnsafeAddress);
    }
    if host.username.trim().is_empty() {
        return Err(DomainError::EmptyField { field: "username" });
    }
    if host.username.chars().any(is_shell_control) {
        return Err(DomainError::UnsafeAddress);
    }
    if !host.default_remote_path.starts_with('/') {
        return Err(DomainError::InvalidRemotePath);
    }
    validate_remote_path(&host.default_remote_path)?;
    if host.port == 0 {
        return Err(DomainError::InvalidPort);
    }
    for address in &host.addresses {
        if address.hostname_or_ip.trim().is_empty() {
            return Err(DomainError::EmptyField { field: "address" });
        }
        if address.port == 0 {
            return Err(DomainError::InvalidPort);
        }
        if address.hostname_or_ip.chars().any(is_shell_control) {
            return Err(DomainError::UnsafeAddress);
        }
    }
    if let Some(jump) = &host.jump {
        if jump.hostname.trim().is_empty() || jump.username.trim().is_empty() {
            return Err(DomainError::EmptyField { field: "jump host" });
        }
        if jump.port == 0 {
            return Err(DomainError::InvalidPort);
        }
        if jump.hostname.chars().any(is_shell_control)
            || jump.username.chars().any(is_shell_control)
        {
            return Err(DomainError::UnsafeAddress);
        }
    }
    Ok(())
}

pub fn validate_project(project: &Project) -> Result<(), DomainError> {
    if project.name.trim().is_empty() {
        return Err(DomainError::EmptyField {
            field: "project.name",
        });
    }
    validate_remote_path(&project.remote_cwd)
}

pub fn validate_action(action: &Action) -> Result<(), DomainError> {
    if action.name.trim().is_empty() {
        return Err(DomainError::EmptyField {
            field: "action.name",
        });
    }
    if action.command.trim().is_empty() {
        return Err(DomainError::EmptyCommand);
    }
    if action
        .cwd
        .as_ref()
        .is_some_and(|cwd| validate_remote_path(cwd).is_err())
    {
        return Err(DomainError::InvalidRemotePath);
    }
    Ok(())
}

pub fn validate_remote_path(path: &str) -> Result<(), DomainError> {
    if !(path == "~" || path.starts_with("~/") || path.starts_with('/'))
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|component| component == "." || component == "..")
    {
        return Err(DomainError::InvalidRemotePath);
    }
    Ok(())
}

#[must_use]
pub fn classify_danger(command: &str) -> DangerLevel {
    // Normalize whitespace so tabs/newlines and repeated spaces cannot evade
    // the server-side policy. This is intentionally conservative: a false
    // positive merely asks for confirmation, while a false negative can
    // destroy a remote workspace.
    let lowered = command.to_ascii_lowercase();
    let normalized = command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let tokens: Vec<_> = normalized.split(' ').collect();
    let rm_recursive_force = tokens.iter().enumerate().any(|(index, token)| {
        *token == "rm"
            && tokens[index + 1..]
                .iter()
                .take(4)
                .any(|flag| flag.starts_with('-') && flag.contains('r'))
            && tokens[index + 1..]
                .iter()
                .take(4)
                .any(|flag| flag.starts_with('-') && flag.contains('f'))
    });
    let dangerous = rm_recursive_force
        || normalized.contains("git reset --hard")
        || normalized.contains("git clean -fd")
        || normalized.contains("git clean -xdf")
        || normalized.contains("mkfs")
        || normalized.contains("shutdown")
        || normalized.contains("reboot")
        || normalized.contains("poweroff")
        || normalized.contains("deploy-prod")
        || normalized.contains("terraform destroy")
        || normalized.contains("kubectl delete")
        || normalized.contains("helm uninstall")
        || normalized.contains("docker system prune")
        || normalized.contains("docker volume prune")
        || (normalized.contains("find ") && normalized.contains(" -delete"))
        || normalized.contains("git push --force")
        || (normalized.contains("curl ") && pipe_to_shell(&normalized))
        || (normalized.contains("wget ") && pipe_to_shell(&normalized))
        || normalized.contains(":(){ :|:& };:")
        || (lowered.contains("dd if=") && lowered.contains(" of=/dev/"));
    if dangerous {
        return DangerLevel::Dangerous;
    }
    // Safe actions are only a UX guardrail, never a shell sandbox. Any shell
    // composition must therefore require an explicit review: command
    // substitution, pipelines, redirections, grouping, escaping and control
    // operators can execute side effects before the apparently harmless outer
    // command is evaluated.
    if contains_shell_composition(command) {
        return DangerLevel::Review;
    }
    if normalized.contains("sudo ")
        || normalized.contains("chmod ")
        || normalized.contains("chown ")
        || normalized.contains("systemctl ")
        || normalized.contains("kill ")
    {
        return DangerLevel::Review;
    }
    // Only a small observational set is confidently safe. Shell commands
    // are open-ended; treating every unknown construct as Safe creates a
    // false sense of protection around scripts, aliases and interpreters.
    let clearly_observational = [
        "pwd",
        "whoami",
        "id",
        "date",
        "uname",
        "hostname",
        "git status",
        "git diff",
        "git log",
        "ls",
        "ll",
        "cat",
        "head",
        "tail",
        "echo",
        "printf",
        "which",
        "command -v",
        "tmux ls",
    ];
    if clearly_observational
        .iter()
        .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix} ")))
    {
        DangerLevel::Safe
    } else {
        DangerLevel::Review
    }
}

fn contains_shell_composition(command: &str) -> bool {
    command.chars().any(|character| {
        matches!(
            character,
            '\r' | '\n' | ';' | '|' | '&' | '>' | '<' | '`' | '$' | '\\' | '(' | ')' | '{' | '}'
        )
    })
}

fn pipe_to_shell(command: &str) -> bool {
    [
        "| sh", "|sh", "| bash", "|bash", "| zsh", "|zsh", "| fish", "|fish", "| python",
        "|python", "| perl", "|perl",
    ]
    .iter()
    .any(|suffix| command.contains(suffix))
}

#[must_use]
pub fn is_shell_control(value: char) -> bool {
    matches!(
        value,
        ' ' | '\t' | '\r' | '\n' | ';' | '|' | '&' | '`' | '$'
    )
}

#[must_use]
pub fn connection_transition(from: ConnectionState, to: ConnectionState) -> bool {
    matches!(
        (from, to),
        (
            ConnectionState::Disconnected | ConnectionState::Failed,
            ConnectionState::ResolvingAddress
        ) | (
            ConnectionState::ResolvingAddress | ConnectionState::Reconnecting,
            ConnectionState::Connecting
        ) | (
            ConnectionState::Connecting,
            ConnectionState::VerifyingHostKey
        ) | (
            ConnectionState::VerifyingHostKey,
            ConnectionState::Authenticating
        ) | (
            ConnectionState::Authenticating | ConnectionState::Reconnecting,
            ConnectionState::Ready
        ) | (
            ConnectionState::Ready,
            ConnectionState::Reconnecting | ConnectionState::Disconnected
        ) | (ConnectionState::Failed, ConnectionState::Disconnected)
    )
}

/// Validates persisted Run lifecycle transitions. Terminal states are final;
/// `Unknown` remains reconcilable because late remote metadata may appear.
#[must_use]
pub fn run_transition(from: RunStatus, to: RunStatus) -> bool {
    if from == to {
        return true;
    }
    matches!(
        (from, to),
        (
            RunStatus::Created,
            RunStatus::Confirming | RunStatus::Queued | RunStatus::Running | RunStatus::Failed
        ) | (
            RunStatus::Confirming,
            RunStatus::Queued | RunStatus::Failed | RunStatus::Cancelled
        ) | (
            RunStatus::Queued,
            RunStatus::Running
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
                | RunStatus::Unknown
        ) | (
            RunStatus::Running,
            RunStatus::Succeeded
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::TimedOut
                | RunStatus::Interrupted
                | RunStatus::Unknown
        ) | (
            RunStatus::Unknown,
            RunStatus::Running | RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        Host {
            id: HostId::new(),
            label: "lab".into(),
            username: "alex".into(),
            port: 22,
            auth_ref: None,
            auth_mode: AuthenticationMode::Password,
            private_key_path: None,
            default_remote_path: "/".into(),
            jump: None,
            addresses: vec![Address {
                id: AddressId::new(),
                kind: AddressKind::Tailscale,
                hostname_or_ip: "100.95.14.21".into(),
                port: 22,
                priority: 10,
                enabled: true,
            }],
            tailscale: None,
            default_runtime: RuntimeKind::Tmux,
        }
    }

    #[test]
    fn validates_host_and_project_paths() {
        assert!(validate_host(&host()).is_ok());
        assert!(validate_remote_path("~/code/project").is_ok());
        assert!(validate_remote_path("~").is_ok());
        assert!(validate_remote_path("relative/path").is_err());
        assert!(validate_remote_path("~/code/../secret").is_err());
        assert!(validate_remote_path("/tmp/./file").is_err());

        let mut invalid_default = host();
        invalid_default.default_remote_path = "~/code/project".into();
        assert_eq!(
            validate_host(&invalid_default),
            Err(DomainError::InvalidRemotePath)
        );
    }

    #[test]
    fn rejects_shell_control_in_address() {
        let mut value = host();
        value.addresses[0].hostname_or_ip = "host;whoami".into();
        assert_eq!(validate_host(&value), Err(DomainError::UnsafeAddress));
    }

    #[test]
    fn classifies_dangerous_commands() {
        assert_eq!(classify_danger("git status"), DangerLevel::Safe);
        assert_eq!(classify_danger("python -c 'print(1)'"), DangerLevel::Review);
        assert_eq!(
            classify_danger("rsync --delete ./ /srv/app"),
            DangerLevel::Review
        );
        assert_eq!(
            classify_danger("sudo systemctl restart app"),
            DangerLevel::Review
        );
        assert_eq!(
            classify_danger("git reset --hard HEAD"),
            DangerLevel::Dangerous
        );
        assert_eq!(
            classify_danger("rm\t-r\t-f\t/tmp/work"),
            DangerLevel::Dangerous
        );
        assert_eq!(
            classify_danger("curl https://example.invalid/x | sh"),
            DangerLevel::Dangerous
        );
        assert_eq!(
            classify_danger("find /tmp -type f -delete"),
            DangerLevel::Dangerous
        );
        assert_eq!(
            classify_danger("wget https://example.invalid/x|bash"),
            DangerLevel::Dangerous
        );
        assert_eq!(
            classify_danger("echo \"$(rm -rf /tmp/work)\""),
            DangerLevel::Review
        );
        assert_eq!(
            classify_danger("git status | tee status.txt"),
            DangerLevel::Review
        );
        assert_eq!(classify_danger("ls > files.txt"), DangerLevel::Review);
        assert_eq!(classify_danger("env"), DangerLevel::Review);
        assert_eq!(classify_danger("printenv"), DangerLevel::Review);
    }

    #[test]
    fn connection_state_machine_rejects_skips() {
        assert!(connection_transition(
            ConnectionState::Disconnected,
            ConnectionState::ResolvingAddress
        ));
        assert!(!connection_transition(
            ConnectionState::Disconnected,
            ConnectionState::Ready
        ));
        assert!(connection_transition(
            ConnectionState::Ready,
            ConnectionState::Reconnecting
        ));
    }

    #[test]
    fn run_state_machine_keeps_final_states_final() {
        assert!(run_transition(RunStatus::Running, RunStatus::Running));
        assert!(run_transition(RunStatus::Running, RunStatus::Succeeded));
        assert!(run_transition(RunStatus::Running, RunStatus::Unknown));
        assert!(run_transition(RunStatus::Running, RunStatus::Interrupted));
        assert!(run_transition(RunStatus::Queued, RunStatus::Interrupted));
        assert!(run_transition(RunStatus::Unknown, RunStatus::Failed));
        assert!(!run_transition(RunStatus::Succeeded, RunStatus::Running));
        assert!(!run_transition(RunStatus::Failed, RunStatus::Succeeded));
        assert!(!run_transition(RunStatus::Interrupted, RunStatus::Running));
        assert!(!run_transition(
            RunStatus::Interrupted,
            RunStatus::Succeeded
        ));
    }

    #[test]
    fn screenshot_fixture_contains_no_secret_material() {
        let raw = include_str!("../../../tests/fixtures/hpc_t_direct.json");
        let parsed = serde_json::from_str::<serde_json::Value>(raw);
        assert!(parsed.is_ok());
        let value = parsed.unwrap_or_default();
        assert_eq!(value["hostname"], "203.0.113.10");
        assert_eq!(value["username"], "testuser");
        assert_eq!(value["session_runtime"], "Herdr");
        assert!(raw.contains("fixture-redacted-tailscale-key"));
        assert!(!raw.contains("tskey-"));
    }
}
