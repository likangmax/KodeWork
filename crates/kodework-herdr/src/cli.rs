#![forbid(unsafe_code)]

//! Herdr CLI adapter: detection, status, capability probing, workspace
//! and agent listing over an SSH exec channel (argv-style commands,
//! shell string concatenation). All JSON parsing is tolerant of unknown
//! fields so protocol drift degrades to diagnostics instead of crashes.

use crate::{parse_schema, HerdrAgentStatus, HerdrCapabilities, HerdrError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Minimum supported Herdr protocol version (from "herdr api schema").
pub const MIN_PROTOCOL_VERSION: u32 = 1;
/// Default per-command deadline.
pub const DEFAULT_CLI_TIMEOUT: Duration = Duration::from_secs(10);

/// One remote command execution result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// Executes commands on the remote host (the SSH exec channel).
#[async_trait::async_trait]
pub trait RemoteExecutor: Send + Sync {
    async fn exec(&self, command: &str, timeout: Duration) -> Result<ExecOutput, HerdrError>;
}

/// Client for remote "herdr" CLI invocations.
pub struct HerdrClient {
    executor: Box<dyn RemoteExecutor>,
    timeout: Duration,
}

impl HerdrClient {
    #[must_use]
    pub fn new(executor: Box<dyn RemoteExecutor>, timeout: Duration) -> Self {
        Self { executor, timeout }
    }

    /// Detects whether herdr is installed and returns its version.
    pub async fn detect(&self) -> Result<String, HerdrError> {
        let output = self.exec_herdr("--version").await?;
        if looks_like_missing_command(&output) {
            return Err(HerdrError::NotInstalled);
        }
        if output.exit_code != 0 {
            return Err(classify_exit(&output));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Err(HerdrError::InvalidResponse);
        }
        Ok(first_line(&text))
    }

    /// "herdr status --json" with tolerant parsing.
    pub async fn status(&self) -> Result<HerdrStatus, HerdrError> {
        let output = self.exec_herdr("status --json").await?;
        if output.exit_code != 0 {
            return Err(classify_exit(&output));
        }
        parse_status(&String::from_utf8_lossy(&output.stdout))
    }

    /// Probes the API schema and derives capabilities.
    pub async fn capabilities(&self) -> Result<HerdrCapabilities, HerdrError> {
        let output = self.exec_herdr("api schema --json").await?;
        if output.exit_code != 0 {
            return Err(classify_exit(&output));
        }
        let schema = parse_schema(&String::from_utf8_lossy(&output.stdout))?;
        // A missing field means "unknown"; an explicit value below the
        // minimum is a hard protocol mismatch.
        match schema.protocol_version {
            None => Ok(HerdrCapabilities {
                protocol_version: None,
                workspace_api: true,
                pane_api: true,
                agent_api: true,
                terminal_control: true,
            }),
            Some(version) if version < MIN_PROTOCOL_VERSION => Err(HerdrError::ProtocolMismatch),
            Some(version) => Ok(HerdrCapabilities {
                protocol_version: Some(version),
                workspace_api: true,
                pane_api: true,
                agent_api: true,
                terminal_control: true,
            }),
        }
    }

    /// Lists workspaces (tolerant JSON).
    pub async fn workspaces(&self) -> Result<Vec<HerdrWorkspace>, HerdrError> {
        let output = self.exec_herdr("workspace list").await?;
        if output.exit_code != 0 {
            return Err(classify_exit(&output));
        }
        parse_workspace_list(&String::from_utf8_lossy(&output.stdout))
    }

    /// Lists live agents (tolerant JSON).
    pub async fn agents(&self) -> Result<Vec<HerdrAgentInfo>, HerdrError> {
        let output = self.exec_herdr("agent list").await?;
        if output.exit_code != 0 {
            return Err(classify_exit(&output));
        }
        parse_agent_list(&String::from_utf8_lossy(&output.stdout))
    }

    /// Run the current Herdr CLI, retrying with well-known user install
    /// locations when an SSH exec shell has a narrower PATH than the user's
    /// interactive shell. The argument string is always a static adapter
    /// constant, never user-controlled input.
    async fn exec_herdr(&self, arguments: &'static str) -> Result<ExecOutput, HerdrError> {
        let direct = format!("herdr {arguments}");
        let output = self.executor.exec(&direct, self.timeout).await?;
        if !looks_like_missing_command(&output) {
            return Ok(output);
        }

        // SSH exec channels commonly skip the user's profile, while Herdr is
        // often installed by Cargo/pipx/Conda and added to PATH there. Ask the
        // user's configured login shell before guessing installation paths.
        // `arguments` is an adapter-owned static string, never user input.
        let login_shell = format!(
            r#"if [ -n "$SHELL" ] && [ -x "$SHELL" ]; then exec "$SHELL" -lc 'exec herdr {arguments}'; fi; exit 127"#
        );
        let output = self.executor.exec(&login_shell, self.timeout).await?;
        if !looks_like_missing_command(&output) {
            return Ok(output);
        }

        let fallback = format!(
            r#"HERDR_BIN=""; for candidate in "$HOME/.cargo/bin/herdr" "$HOME/.local/bin/herdr" "$HOME/bin/herdr" "$HOME/.local/share/pipx/venvs/herdr/bin/herdr" "$HOME/.local/share/uv/tools/herdr/bin/herdr" "$HOME"/.conda/envs/*/bin/herdr "$HOME"/miniconda3/envs/*/bin/herdr "$HOME"/anaconda3/envs/*/bin/herdr "/usr/local/bin/herdr" "/opt/herdr/bin/herdr"; do if [ -x "$candidate" ]; then HERDR_BIN="$candidate"; break; fi; done; if [ -z "$HERDR_BIN" ]; then exit 127; fi; exec "$HERDR_BIN" {arguments}"#
        );
        self.executor.exec(&fallback, self.timeout).await
    }

    /// Probes the remote herdr control socket path. Checks the
    /// HERDR_SOCKET environment variable and common run directories;
    /// returns None when herdr has no reachable socket.
    pub async fn socket_path(&self) -> Result<Option<String>, HerdrError> {
        let command = r#"printf '%s\n' "$HERDR_SOCKET"; find "$HOME/.herdr" -name '*.sock' 2>/dev/null | head -2; herdr status --json 2>/dev/null | head -c 2000"#;
        let output = self.executor.exec(command, self.timeout).await?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut candidates: Vec<String> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('{') {
                continue;
            }
            if line.starts_with('/') || line.starts_with('~') {
                candidates.push(line.to_string());
            }
        }
        candidates.dedup();
        Ok(candidates.into_iter().next())
    }

    pub async fn has_socat(&self) -> Result<bool, HerdrError> {
        let output = self
            .executor
            .exec(
                "command -v socat >/dev/null 2>&1 && echo yes || echo no",
                self.timeout,
            )
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout).contains("yes"))
    }
}

/// Tolerant status model (all fields optional).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrStatus {
    #[serde(default)]
    pub server: Option<HerdrServerStatus>,
    #[serde(default)]
    pub protocol_version: Option<u32>,
    #[serde(default)]
    pub workspaces: Option<Vec<HerdrWorkspace>>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrServerStatus {
    #[serde(default)]
    pub running: Option<bool>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub protocol_version: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrWorkspace {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tabs: Option<Vec<HerdrTab>>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrTab {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub panes: Option<Vec<HerdrPane>>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrPane {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<HerdrAgentState>,
}

/// One live agent as reported by `herdr agent list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HerdrAgentInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    #[serde(alias = "agent")]
    pub kind: Option<String>,
    #[serde(default)]
    #[serde(alias = "agent_status")]
    pub status: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
}

impl HerdrAgentInfo {
    /// Maps the raw status string to the domain enum; unknown values
    /// map to Unknown without failing the parse.
    #[must_use]
    pub fn domain_status(&self) -> HerdrAgentStatus {
        match self.status.as_deref().unwrap_or("") {
            "idle" => HerdrAgentStatus::Idle,
            "working" => HerdrAgentStatus::Working,
            "blocked" => HerdrAgentStatus::Blocked,
            "done" => HerdrAgentStatus::Done,
            _ => HerdrAgentStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HerdrAgentState {
    #[serde(default)]
    pub status: Option<String>,
}

impl HerdrAgentState {
    /// Maps a raw agent status string to the domain enum; unknown values
    /// map to Unknown without failing the parse.
    #[must_use]
    pub fn domain_status(&self) -> HerdrAgentStatus {
        match self.status.as_deref().unwrap_or("") {
            "idle" => HerdrAgentStatus::Idle,
            "working" => HerdrAgentStatus::Working,
            "blocked" => HerdrAgentStatus::Blocked,
            "done" => HerdrAgentStatus::Done,
            _ => HerdrAgentStatus::Unknown,
        }
    }
}

/// Parses a herdr CLI JSON response, tolerating both bare payloads and
/// the `{"result": ..., "error": ...}` envelope. Unknown fields are
/// ignored so protocol drift degrades to diagnostics, not crashes.
fn parse_enveloped<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, HerdrError> {
    if let Ok(value) = serde_json::from_str::<T>(json) {
        return Ok(value);
    }
    #[derive(serde::Deserialize)]
    struct Envelope<T> {
        result: Option<T>,
        #[serde(default)]
        error: Option<String>,
    }
    let envelope: Envelope<T> =
        serde_json::from_str(json).map_err(|_| HerdrError::InvalidResponse)?;
    if let Some(error) = envelope.error.filter(|message| !message.is_empty()) {
        return Err(HerdrError::Executor(error));
    }
    envelope.result.ok_or(HerdrError::InvalidResponse)
}

pub fn parse_status(json: &str) -> Result<HerdrStatus, HerdrError> {
    parse_enveloped(json)
}

pub fn parse_workspace_list(json: &str) -> Result<Vec<HerdrWorkspace>, HerdrError> {
    parse_list_payload(json, "workspaces")
}

pub fn parse_agent_list(json: &str) -> Result<Vec<HerdrAgentInfo>, HerdrError> {
    parse_list_payload(json, "agents")
}

/// Herdr 0.6+ wraps list results as `{"result":{"agents":[...]}}`, while
/// older releases and early adapters returned a bare array or put the array
/// directly in `result`. Accept all three shapes so a server upgrade does not
/// make the Windows client falsely report that Herdr is missing.
fn parse_list_payload<T: serde::de::DeserializeOwned>(
    json: &str,
    field: &str,
) -> Result<Vec<T>, HerdrError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| HerdrError::InvalidResponse)?;
    if let Some(error) = value
        .get("error")
        .and_then(|error| error.as_str())
        .filter(|error| !error.is_empty())
    {
        return Err(HerdrError::Executor(error.to_string()));
    }
    let payload = value.get("result").unwrap_or(&value);
    let list = payload.get(field).unwrap_or(payload).clone();
    serde_json::from_value(list).map_err(|_| HerdrError::InvalidResponse)
}

fn classify_exit(output: &ExecOutput) -> HerdrError {
    if looks_like_missing_command(output) {
        return HerdrError::NotInstalled;
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined =
        format!("{} {}", stderr, String::from_utf8_lossy(&output.stdout)).to_ascii_lowercase();
    if combined.contains("server is not running") || combined.contains("not running") {
        return HerdrError::ServerNotRunning;
    }
    HerdrError::CommandFailed {
        exit_code: output.exit_code,
        stderr,
    }
}

fn looks_like_missing_command(output: &ExecOutput) -> bool {
    let combined = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    )
    .to_ascii_lowercase();
    (output.exit_code == 127 && combined.trim().is_empty())
        || combined.contains("command not found")
        || combined.contains("no such file or directory")
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}
