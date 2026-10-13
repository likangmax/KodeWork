#![forbid(unsafe_code)]

//! Pure Action/Run policy, shell quoting, durable metadata and probe construction.
//! Deliberately independent from SSH session generations, PTY, and event pumps.

use kodework_domain::{validate_remote_path, Action, ActionMode, RunId};
use kodework_ssh::connection::CommandExecutionError;

pub(super) fn build_run_script(command: &str, run_key: &str) -> Result<String, String> {
    let quoted_command = shell_quote(command)?;
    Ok(format!(
        "umask 077 || exit 125; base=\"$HOME/.cache/kodework/runs/{run_key}\"; if ! mkdir -p -- \"$base\"; then exit 125; fi; started_tmp=\"$base/started_at_s.tmp.$$\"; if ! date +%s > \"$started_tmp\" || ! mv -f -- \"$started_tmp\" \"$base/started_at_s\"; then rm -f -- \"$started_tmp\"; exit 125; fi; sh -lc {quoted_command}; code=$?; finished_tmp=\"$base/finished_at_s.tmp.$$\"; exit_tmp=\"$base/exit_code.tmp.$$\"; if ! date +%s > \"$finished_tmp\" || ! mv -f -- \"$finished_tmp\" \"$base/finished_at_s\"; then rm -f -- \"$finished_tmp\" \"$exit_tmp\"; exit 125; fi; if ! printf '%s\\n' \"$code\" > \"$exit_tmp\" || ! mv -f -- \"$exit_tmp\" \"$base/exit_code\"; then rm -f -- \"$exit_tmp\"; exit 125; fi; exit \"$code\""
    ))
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
    pub(super) fn before_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            dispatched: false,
            timed_out: false,
        }
    }

    pub(super) fn from_command(error: CommandExecutionError) -> Self {
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

pub(super) fn parse_epoch_seconds(value: Option<&str>) -> Option<u64> {
    value?.parse::<u64>().ok()?.checked_mul(1_000)
}

pub(super) fn build_remote_run_probe_command(
    requests: &[RemoteRunProbeRequest],
) -> Result<String, String> {
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
        // Liveness must come from the pane, not from the session: with
        // `remain-on-exit on` (a common tmux setting) the session outlives the
        // script, so `has-session` alone reported a dead run as Running forever.
        // Only an explicitly live pane (`pane_dead` = 0) counts as running;
        // anything else - a dead pane, no output, or a tmux error - is Unknown,
        // which keeps the documented "started is not success" contract.
        command.push_str(&format!(
            "id='{id}'; base=\"$HOME/.cache/kodework/runs/$id\"; if [ -f \"$base/exit_code\" ]; then printf '%s\\tcompleted\\t%s\\t%s\\t%s\\n' \"$id\" \"$(cat -- \"$base/exit_code\" 2>/dev/null || true)\" \"$(cat -- \"$base/started_at_s\" 2>/dev/null || true)\" \"$(cat -- \"$base/finished_at_s\" 2>/dev/null || true)\"; elif [ '{mode}' = 'background' ] && tmux has-session -t \"kodework-run-$id\" >/dev/null 2>&1; then dead=$(tmux list-panes -t \"kodework-run-$id\" -F '#{{pane_dead}}' 2>/dev/null | head -n 1); if [ \"$dead\" = \"0\" ]; then printf '%s\\trunning\\n' \"$id\"; else printf '%s\\tunknown\\n' \"$id\"; fi; else printf '%s\\tunknown\\n' \"$id\"; fi; "
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
pub(super) fn shell_quote(value: &str) -> Result<String, String> {
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

/// Builds the remote shell line for an action: optional quoted env prefix,
/// optional quoted cwd, then the intentional raw command text.
pub(super) fn build_action_command(action: &Action) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
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

    /// Hostile payloads must survive the quoting rule byte for byte. Running the
    /// real interpreter is the only way to prove that, because the assertion is
    /// about shell semantics rather than about string contents.
    ///
    /// A POSIX shell is required: the Unix CI jobs always have one, so the
    /// invariant is enforced there. On Windows the test runs when a working
    /// `sh`/`bash` is reachable (for example through WSL) and otherwise reports
    /// that it could not run instead of pretending to pass.
    #[test]
    fn shell_quoting_round_trips_through_a_real_posix_shell() {
        let Some(shell) = working_posix_shell() else {
            // A POSIX shell is required on Unix, which is the enforcement point
            // for this invariant. On Windows it is optional: the test runs when
            // one is reachable (for example Git's sh) and reports that it could
            // not run otherwise, rather than pretending to pass.
            #[cfg(windows)]
            {
                eprintln!("skipped: no POSIX shell available on this host");
                return;
            }
            #[cfg(not(windows))]
            {
                unreachable!("a POSIX shell is required to verify shell quoting here");
            }
        };

        let payloads = [
            "simple",
            "with space",
            "single'quote",
            "double\"quote",
            "back\\slash",
            "$(whoami)",
            "`id`",
            "semi;colon",
            "pipe|pipe",
            "redirect>file",
            "glob*?[a]",
            "unicode-\u{4e2d}\u{6587}-\u{2713}",
            "!history",
            "#hash",
            "~tilde",
            "-leading-dash",
            "tab\there",
            "trailing-space ",
            "''",
            "\"\"",
        ];
        for payload in payloads {
            let quoted = shell_quote(payload)
                .unwrap_or_else(|error| unreachable!("{payload:?} should be quotable: {error}"));
            let output = std::process::Command::new(shell)
                .arg("-c")
                .arg(format!("printf %s {quoted}"))
                .output()
                .unwrap_or_else(|error| unreachable!("{shell} -c: {error}"));
            assert!(
                output.status.success(),
                "{shell} rejected the quoting for {payload:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                payload,
                "round trip changed {payload:?}"
            );
        }

        // Control characters cannot be represented safely in a single shell word
        // and must be refused rather than escaped.
        for payload in ["line\nfeed", "carriage\rreturn", "nul\0byte"] {
            assert!(
                shell_quote(payload).is_err(),
                "{payload:?} must be rejected, not quoted"
            );
        }
    }

    /// The first shell that actually runs, so a broken stub on `PATH` cannot make
    /// the quoting test vacuous.
    fn working_posix_shell() -> Option<&'static str> {
        ["sh", "bash"].into_iter().find(|candidate| {
            std::process::Command::new(candidate)
                .arg("-c")
                .arg("printf ok")
                .output()
                .map(|output| output.status.success() && output.stdout == b"ok")
                .unwrap_or(false)
        })
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
        // A tmux session that survives its command (`remain-on-exit on`) must not
        // be reported as a running run: only a live pane is evidence.
        assert!(script.contains("#{pane_dead}"));
        assert!(script.contains("[ \"$dead\" = \"0\" ]"));
        assert!(!script.contains("[ -f \"$base/started_at_s\" ] ||"));
        assert!(script.contains("else printf '%s\\tunknown\\n'"));
    }

    #[test]
    fn probe_gates_tmux_liveness_on_the_requested_mode() {
        let quick = build_remote_run_probe_command(&[RemoteRunProbeRequest {
            run_id: RunId::new(),
            mode: ActionMode::Quick,
        }])
        .unwrap_or_else(|error| unreachable!("safe UUIDs must build: {error}"));
        let background = build_remote_run_probe_command(&[RemoteRunProbeRequest {
            run_id: RunId::new(),
            mode: ActionMode::Background,
        }])
        .unwrap_or_else(|error| unreachable!("safe UUIDs must build: {error}"));

        assert!(quick.contains("[ 'quick' = 'background' ] && tmux has-session"));
        assert!(background.contains("[ 'background' = 'background' ] && tmux has-session"));
        // No tmux liveness check may be reachable without the mode guard: a
        // Quick run is awaited to completion, so a leftover tmux session must
        // never be able to turn an unresolved run into "running".
        for (label, script) in [("quick", &quick), ("background", &background)] {
            let total = script.matches("tmux has-session").count();
            let guarded = script.matches("] && tmux has-session").count();
            assert_eq!(
                guarded, total,
                "{label}: every tmux check must be mode-gated"
            );
        }
    }
}
