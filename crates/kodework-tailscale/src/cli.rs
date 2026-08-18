#![forbid(unsafe_code)]

//! tailscale CLI adapter with bounded execution: argv-based invocation,
//! deadline, output size cap and typed failure classification. No secret
//! material is ever passed as an argument.

use crate::{parse_status, TailscaleError, TailscaleStatus};
use std::path::PathBuf;
use std::time::Duration;

/// Cap on `status --json` output; oversized output is treated as an error
/// rather than being buffered unboundedly.
pub const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
/// Default CLI deadline.
pub const DEFAULT_CLI_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs a tailscale command and returns `(exit_code, stdout, stderr)`.
#[async_trait::async_trait]
pub trait TailscaleRunner: Send + Sync {
    async fn run(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError>;
}

/// Spawns the real `tailscale` executable with bounded stdout/stderr.
pub struct ProcessTailscaleRunner {
    executable: PathBuf,
}

impl ProcessTailscaleRunner {
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }
}

#[async_trait::async_trait]
impl TailscaleRunner for ProcessTailscaleRunner {
    async fn run(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;

        let mut command = tokio::process::Command::new(&self.executable);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| TailscaleError::Spawn(error.to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TailscaleError::Spawn("stdout pipe unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| TailscaleError::Spawn("stderr pipe unavailable".into()))?;

        let (stdout_result, stderr_result) = tokio::time::timeout(timeout, async {
            let stdout_task = async {
                let mut bytes = Vec::new();
                stdout
                    .take(MAX_OUTPUT_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map(|_| bytes)
            };
            let stderr_task = async {
                let mut bytes = Vec::new();
                stderr
                    .take(MAX_OUTPUT_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map(|_| bytes)
            };
            tokio::join!(stdout_task, stderr_task)
        })
        .await
        .map_err(|_| {
            child.start_kill().ok();
            TailscaleError::Timeout
        })?;
        let stdout =
            stdout_result.map_err(|error| TailscaleError::OutputRead(error.to_string()))?;
        let stderr =
            stderr_result.map_err(|error| TailscaleError::OutputRead(error.to_string()))?;
        if stdout.len() as u64 > MAX_OUTPUT_BYTES || stderr.len() as u64 > MAX_OUTPUT_BYTES {
            child.start_kill().ok();
            return Err(TailscaleError::OutputTooLarge(MAX_OUTPUT_BYTES));
        }

        // The wait must be bounded too: a wedged child whose pipes are
        // drained (or capped) would otherwise hang the caller forever.
        let status = tokio::time::timeout(timeout, child.wait())
            .await
            .map_err(|_| {
                child.start_kill().ok();
                TailscaleError::Timeout
            })?
            .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
        let exit_code = status.code().unwrap_or(-1);
        Ok((exit_code, stdout, stderr))
    }
}

/// High-level CLI wrapper.
pub struct TailscaleCli {
    runner: Box<dyn TailscaleRunner>,
    timeout: Duration,
    socket: Option<PathBuf>,
}

impl TailscaleCli {
    #[must_use]
    pub fn new(runner: Box<dyn TailscaleRunner>, timeout: Duration) -> Self {
        Self {
            runner,
            timeout,
            socket: None,
        }
    }

    /// Creates a CLI backed by the real executable.
    #[must_use]
    pub fn from_executable(executable: impl Into<PathBuf>) -> Self {
        Self::new(
            Box::new(ProcessTailscaleRunner::new(executable)),
            DEFAULT_CLI_TIMEOUT,
        )
    }

    /// Uses a specific tailscaled control socket. This is required for a
    /// managed userspace daemon so it cannot accidentally talk to the
    /// machine-wide Windows service.
    #[must_use]
    pub fn with_socket(mut self, socket: impl Into<PathBuf>) -> Self {
        self.socket = Some(socket.into());
        self
    }

    /// Overrides the process deadline for short-lived health probes.  A
    /// userspace daemon whose control pipe is still coming up should be
    /// retried quickly instead of letting one `status` child consume the
    /// entire runtime startup budget.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn command_args<'a>(&'a self, args: &'a [&'a str], owned: &'a mut Vec<String>) -> Vec<&'a str> {
        owned.clear();
        let mut refs = Vec::with_capacity(args.len() + 2);
        if let Some(socket) = &self.socket {
            owned.push("--socket".to_string());
            owned.push(socket.to_string_lossy().into_owned());
            refs.push(owned[0].as_str());
            refs.push(owned[1].as_str());
        }
        refs.extend(args.iter().copied());
        refs
    }

    async fn run(&self, args: &[&str]) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
        self.run_with_timeout(args, self.timeout).await
    }

    async fn run_with_timeout(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
        let mut owned = Vec::new();
        let command = self.command_args(args, &mut owned);
        self.runner.run(&command, timeout).await
    }

    /// `tailscale status --json` with classification of daemon failures.
    pub async fn status(&self) -> Result<TailscaleStatus, TailscaleError> {
        let (exit_code, stdout, stderr) = self.run(&["status", "--json"]).await?;
        if exit_code != 0 {
            let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
            let is_daemon_issue = stderr_text.to_ascii_lowercase().contains("daemon")
                || stderr_text.to_ascii_lowercase().contains("connect");
            return if is_daemon_issue {
                Err(TailscaleError::DaemonUnavailable(
                    if stderr_text.is_empty() {
                        format!("tailscale exited with code {exit_code}")
                    } else {
                        stderr_text
                    },
                ))
            } else {
                Err(TailscaleError::CommandFailed {
                    exit_code,
                    stderr: stderr_text,
                })
            };
        }
        parse_status(&String::from_utf8_lossy(&stdout))
    }

    /// Runs `tailscale up` with an auth-key file reference. The secret value
    /// itself never enters argv; callers own the temporary file lifecycle.
    pub async fn up_with_auth_key_file(
        &self,
        auth_key_file: &std::path::Path,
        timeout: Duration,
    ) -> Result<(), TailscaleError> {
        let auth_arg = format!("file:{}", auth_key_file.display());
        let timeout_arg = format!("{}s", timeout.as_secs().max(1));
        let args = [
            "up",
            "--auth-key",
            auth_arg.as_str(),
            "--unattended",
            "--timeout",
            timeout_arg.as_str(),
        ];
        // The CLI's own --timeout is allowed to elapse before it reports an
        // error. Give the child process a small cleanup/serialization margin
        // instead of killing a registration that is still completing.
        let process_timeout = timeout.saturating_add(Duration::from_secs(5));
        let (exit_code, _stdout, stderr) = self.run_with_timeout(&args, process_timeout).await?;
        if exit_code == 0 {
            return Ok(());
        }
        Err(TailscaleError::CommandFailed {
            exit_code,
            stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
        })
    }
}
