#![forbid(unsafe_code)]

//! Managed Tailscale lifecycle for the optional userspace backend.
//!
//! The system-daemon mode intentionally remains read-only: Kodework observes
//! the user's existing Tailscale installation and never changes its account.
//! Embedded mode owns a private `tailscaled` child, a private state file and a
//! private control socket. Auth keys are accepted only as bytes, materialized
//! in a short-lived `file:` handoff, and removed immediately after `tailscale
//! up` returns; they are never put in argv or logs.

use crate::cli::TailscaleCli;
use crate::{TailscaleError, TailscaleStatus};
use kodework_domain::{TailscaleConfig, TailscaleMode};
use kodework_network::ProxySpec;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const STATUS_TIMEOUT: Duration = Duration::from_secs(2);
// Long enough to cover the normal user pause between app launch and Connect,
// while still forcing periodic peer revalidation during a long-lived session.
const STATUS_CACHE_TTL: Duration = Duration::from_secs(30);

struct CachedStatus {
    state_path: PathBuf,
    captured_at: Instant,
    status: TailscaleStatus,
}

struct RuntimeInner {
    child: Option<Child>,
    socket: Option<PathBuf>,
    state_path: Option<PathBuf>,
    state_lock: Option<File>,
    lease_path: Option<PathBuf>,
    cached_status: Option<CachedStatus>,
}

impl Drop for RuntimeInner {
    fn drop(&mut self) {
        if let Some(path) = self.lease_path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RuntimeLease {
    pid: u32,
    socket: PathBuf,
}

/// Clone-safe controller for one Kodework-managed userspace daemon.
#[derive(Clone)]
pub struct TailscaleRuntime {
    executable: PathBuf,
    fallback_state_root: PathBuf,
    lifecycle: Arc<Mutex<()>>,
    inner: Arc<Mutex<RuntimeInner>>,
}

impl TailscaleRuntime {
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>, fallback_state_root: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            fallback_state_root: fallback_state_root.into(),
            lifecycle: Arc::new(Mutex::new(())),
            inner: Arc::new(Mutex::new(RuntimeInner {
                child: None,
                socket: None,
                state_path: None,
                state_lock: None,
                lease_path: None,
                cached_status: None,
            })),
        }
    }

    #[must_use]
    pub fn component_paths(&self) -> (&Path, PathBuf) {
        (&self.executable, daemon_executable(&self.executable))
    }

    /// Returns status through the configured backend. Embedded mode only
    /// succeeds after `ensure` has started the private daemon.
    pub async fn status_for_config(
        &self,
        config: Option<&TailscaleConfig>,
    ) -> Result<TailscaleStatus, TailscaleError> {
        if config
            .is_some_and(|value| value.enabled && value.mode == TailscaleMode::EmbeddedUserspace)
        {
            let config = config.ok_or_else(|| {
                TailscaleError::DaemonUnavailable("embedded configuration is missing".into())
            })?;
            let expected_state = self.state_path(config)?;
            let (socket, active_state, cached) = {
                let mut inner = self.inner.lock().await;
                let child_alive = inner
                    .child
                    .as_mut()
                    .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
                let cached = child_alive
                    .then(|| inner.cached_status.as_ref())
                    .flatten()
                    .filter(|cached| {
                        cached.state_path == expected_state
                            && cached.captured_at.elapsed() <= STATUS_CACHE_TTL
                    })
                    .map(|cached| cached.status.clone());
                (inner.socket.clone(), inner.state_path.clone(), cached)
            };
            if let Some(status) = cached {
                return Ok(status);
            }
            let socket = socket.ok_or_else(|| {
                TailscaleError::DaemonUnavailable(
                    "managed userspace daemon has not been started".into(),
                )
            })?;
            if active_state.as_ref() != Some(&expected_state) {
                return Err(TailscaleError::DaemonUnavailable(
                    "another embedded Tailscale state is already active".into(),
                ));
            }
            let status = self.embedded_cli(socket).status().await?;
            self.cache_status(&expected_state, &status).await;
            return Ok(status);
        }
        TailscaleCli::from_executable(self.executable.clone())
            .status()
            .await
    }

    /// Builds a process transport for one userspace target. `tailscale nc`
    /// speaks raw TCP over the managed daemon, so russh can retain its normal
    /// SSH authentication/host-key behavior without requiring a TUN device.
    #[must_use]
    pub async fn proxy_spec(
        &self,
        config: Option<&TailscaleConfig>,
        target: &str,
        port: u16,
    ) -> Option<ProxySpec> {
        if !config
            .is_some_and(|value| value.enabled && value.mode == TailscaleMode::EmbeddedUserspace)
        {
            return None;
        }
        let config = config?;
        let expected_state = self.state_path(config).ok()?;
        let inner = self.inner.lock().await;
        if inner.state_path.as_ref() != Some(&expected_state) {
            return None;
        }
        let socket = inner.socket.clone()?;
        Some(ProxySpec {
            program: self.executable.to_string_lossy().into_owned(),
            args: vec![
                "--socket".into(),
                socket.to_string_lossy().into_owned(),
                "nc".into(),
                target.into(),
                port.to_string(),
            ],
        })
    }

    /// Ensures the requested backend is usable. SystemDaemon is read-only;
    /// EmbeddedUserspace starts and optionally authenticates the private child.
    pub async fn ensure(
        &self,
        config: &TailscaleConfig,
        auth_key: Option<&[u8]>,
    ) -> Result<TailscaleStatus, TailscaleError> {
        if !config.enabled || config.mode == TailscaleMode::Disabled {
            return Err(TailscaleError::DaemonUnavailable(
                "tailscale integration is disabled".into(),
            ));
        }
        if config.mode == TailscaleMode::SystemDaemon {
            return TailscaleCli::from_executable(self.executable.clone())
                .status()
                .await;
        }

        let _lifecycle = self.lifecycle.lock().await;
        let state_path = self.state_path(config)?;
        let socket = self.ensure_child(&state_path).await?;
        if let Some(status) = self.cached_status(&state_path).await {
            return Ok(status);
        }
        let cli = self.embedded_cli(socket);

        // A freshly spawned tailscaled may need a short interval before its
        // named-pipe control socket accepts requests.  Do not race `tailscale
        // up` against that startup window: wait only for daemon reachability,
        // then let the normal authentication/state machine handle NeedsLogin.
        let status = wait_until_reachable(&cli).await?;
        if is_running(&status) {
            self.cache_status(&state_path, &status).await;
            return Ok(status);
        }

        let Some(auth_key) = auth_key else {
            return Err(TailscaleError::DaemonUnavailable(
                "managed userspace daemon needs a Tailscale auth key".into(),
            ));
        };
        let auth_key = Zeroizing::new(auth_key.to_vec());
        validate_auth_key(&auth_key)?;
        let auth_file = state_path.with_extension("auth.tmp");
        write_auth_file(&auth_file, &auth_key)?;
        let _auth_file_guard = AuthFileGuard(auth_file.clone());
        cli.up_with_auth_key_file(&auth_file, STARTUP_TIMEOUT)
            .await?;
        let status = wait_until_running(&cli).await?;
        self.cache_status(&state_path, &status).await;
        Ok(status)
    }

    /// Stops the daemon owned by this application instance and releases its
    /// lease/lock.  This is called during a normal desktop shutdown so the
    /// next launch does not have to perform the comparatively expensive stale
    /// PID inspection through PowerShell.
    pub async fn shutdown(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let child = {
            let mut inner = self.inner.lock().await;
            inner.cached_status = None;
            inner.socket = None;
            inner.state_path = None;
            inner.state_lock = None;
            if let Some(path) = inner.lease_path.take() {
                let _ = std::fs::remove_file(path);
            }
            inner.child.take()
        };
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
    }

    fn embedded_cli(&self, socket: PathBuf) -> TailscaleCli {
        TailscaleCli::from_executable(self.executable.clone())
            .with_socket(socket)
            .with_timeout(STATUS_TIMEOUT)
    }

    async fn cache_status(&self, state_path: &Path, status: &TailscaleStatus) {
        self.inner.lock().await.cached_status = Some(CachedStatus {
            state_path: state_path.to_path_buf(),
            captured_at: Instant::now(),
            status: status.clone(),
        });
    }

    async fn cached_status(&self, state_path: &Path) -> Option<TailscaleStatus> {
        self.inner
            .lock()
            .await
            .cached_status
            .as_ref()
            .filter(|cached| {
                cached.state_path == state_path && cached.captured_at.elapsed() <= STATUS_CACHE_TTL
            })
            .map(|cached| cached.status.clone())
    }

    fn state_path(&self, config: &TailscaleConfig) -> Result<PathBuf, TailscaleError> {
        let path = config
            .state_dir
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.fallback_state_root.join("tailscale.state"));
        if !path.is_absolute() {
            return Err(TailscaleError::InvalidStatePath);
        }
        Ok(path)
    }

    async fn ensure_child(&self, state_path: &Path) -> Result<PathBuf, TailscaleError> {
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
        }
        let socket = socket_path(state_path);
        let mut inner = self.inner.lock().await;
        if let Some(child) = inner.child.as_mut() {
            match child.try_wait() {
                Ok(None) => {
                    if inner
                        .state_path
                        .as_ref()
                        .is_some_and(|active| active != state_path)
                    {
                        return Err(TailscaleError::DaemonUnavailable(
                            "another embedded Tailscale state is already active".into(),
                        ));
                    }
                    inner.socket = Some(socket.clone());
                    return Ok(socket);
                }
                Ok(Some(_)) | Err(_) => {
                    if let Some(path) = inner.lease_path.take() {
                        let _ = std::fs::remove_file(path);
                    }
                    inner.child = None;
                    inner.socket = None;
                    inner.state_path = None;
                    inner.state_lock = None;
                    inner.cached_status = None;
                }
            }
        }
        let state_lock = acquire_state_lock(state_path)?;
        cleanup_stale_daemon(state_path).await?;
        let daemon = daemon_executable(&self.executable);
        let mut command = Command::new(daemon);
        command
            .args([
                "--tun=userspace-networking".to_string(),
                format!("--state={}", state_path.display()),
                format!("--socket={}", socket.display()),
                "--socks5-server=127.0.0.1:0".to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        hide_console(&mut command);
        let child = command
            .spawn()
            .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
        let lease_path = state_path.with_extension("runtime.json");
        if let Some(pid) = child.id() {
            write_runtime_lease(&lease_path, pid, &socket)?;
            inner.lease_path = Some(lease_path);
        }
        inner.child = Some(child);
        inner.socket = Some(socket.clone());
        inner.state_path = Some(state_path.to_path_buf());
        inner.state_lock = Some(state_lock);
        inner.cached_status = None;
        Ok(socket)
    }
}

impl Drop for TailscaleRuntime {
    fn drop(&mut self) {
        // `kill_on_drop` handles the final owner. The explicit child is kept
        // in the mutex so every clone observes the same process lifetime.
    }
}

fn daemon_executable(executable: &Path) -> PathBuf {
    // Derive the executable suffix from the configured CLI path rather than
    // the host compiling the crate. This keeps path resolution correct in
    // portable tests and when a Windows sidecar path is inspected off-host.
    let suffix = executable
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let daemon_name = format!("tailscaled{suffix}");
    if executable.components().count() > 1 {
        executable.with_file_name(daemon_name)
    } else {
        PathBuf::from(daemon_name)
    }
}

fn socket_path(state_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        // Windows tailscaled uses a named pipe rather than a filesystem Unix
        // socket. Scope it to this application process so a daemon left behind
        // by an abnormal previous exit cannot be mistaken for the daemon we
        // just launched. Clones in this process still resolve the same pipe.
        let mut hash = 0xcbf29ce484222325u64;
        for byte in state_path.to_string_lossy().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        PathBuf::from(format!(
            r"\\.\pipe\Kodework-Tailscale-{hash:016x}-{}",
            std::process::id()
        ))
    }
    #[cfg(not(windows))]
    {
        state_path.with_extension("sock")
    }
}

fn acquire_state_lock(state_path: &Path) -> Result<File, TailscaleError> {
    let lock_path = state_path.with_extension("lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // No sharing: Windows releases the lock automatically if the owning
        // process crashes, so a stale marker file never blocks future starts.
        options.share_mode(0);
    }
    options.open(&lock_path).map_err(|error| {
        TailscaleError::DaemonUnavailable(format!(
            "embedded Tailscale state is already in use by another Kodework process ({error}); quit every old tray instance and retry"
        ))
    })
}

fn write_runtime_lease(path: &Path, pid: u32, socket: &Path) -> Result<(), TailscaleError> {
    let data = serde_json::to_vec(&RuntimeLease {
        pid,
        socket: socket.to_path_buf(),
    })
    .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
    std::fs::write(path, data).map_err(|error| TailscaleError::Spawn(error.to_string()))
}

/// Reaps a daemon left by an abnormal previous UI termination. The state
/// lock proves no live Kodework instance currently owns this state. On
/// Windows we additionally verify the recorded PID's command line contains
/// both our exact state file and private pipe before terminating anything.
async fn cleanup_stale_daemon(state_path: &Path) -> Result<(), TailscaleError> {
    let lease_path = state_path.with_extension("runtime.json");
    let Ok(data) = std::fs::read(&lease_path) else {
        return Ok(());
    };
    let Ok(lease) = serde_json::from_slice::<RuntimeLease>(&data) else {
        let _ = std::fs::remove_file(lease_path);
        return Ok(());
    };

    #[cfg(not(windows))]
    let _ = &lease;

    #[cfg(windows)]
    {
        let query = format!(
            "$p=Get-CimInstance Win32_Process -Filter 'ProcessId = {}' -ErrorAction SilentlyContinue; if ($p) {{ [Console]::Out.Write($p.CommandLine) }}",
            lease.pid
        );
        let mut inspect = Command::new("powershell.exe");
        inspect
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &query,
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut inspect);
        let output = tokio::time::timeout(Duration::from_secs(5), inspect.output())
            .await
            .map_err(|_| TailscaleError::Spawn("stale daemon inspection timed out".into()))?
            .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
        let command_line = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        let expected_state = format!("--state={}", state_path.display()).to_ascii_lowercase();
        let expected_socket = format!("--socket={}", lease.socket.display()).to_ascii_lowercase();
        if output.status.success()
            && command_line.contains("tailscaled.exe")
            && command_line.contains(&expected_state)
            && command_line.contains(&expected_socket)
        {
            let mut kill = Command::new("taskkill.exe");
            let pid = lease.pid.to_string();
            kill.args(["/PID", &pid, "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            hide_console(&mut kill);
            let status = tokio::time::timeout(Duration::from_secs(5), kill.status())
                .await
                .map_err(|_| TailscaleError::Spawn("stale daemon cleanup timed out".into()))?
                .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
            if !status.success() {
                return Err(TailscaleError::DaemonUnavailable(
                    "a stale embedded Tailscale daemon could not be stopped".into(),
                ));
            }
        }
    }

    let _ = std::fs::remove_file(lease_path);
    Ok(())
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn validate_auth_key(auth_key: &[u8]) -> Result<(), TailscaleError> {
    if auth_key.is_empty() || auth_key.iter().any(u8::is_ascii_whitespace) {
        return Err(TailscaleError::CommandFailed {
            exit_code: -1,
            stderr: "Tailscale auth key is empty or contains whitespace".into(),
        });
    }
    Ok(())
}

fn write_auth_file(path: &Path, auth_key: &[u8]) -> Result<(), TailscaleError> {
    use std::io::Write;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| TailscaleError::Spawn(error.to_string()))?;
    let result = file
        .write_all(auth_key)
        .and_then(|_| file.flush())
        .map_err(|error| TailscaleError::Spawn(error.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

struct AuthFileGuard(PathBuf);

impl Drop for AuthFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn wait_until_running(cli: &TailscaleCli) -> Result<TailscaleStatus, TailscaleError> {
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
    loop {
        let message = match cli.status().await {
            Ok(status) if is_running(&status) => return Ok(status),
            Ok(status) => status
                .backend_state
                .clone()
                .unwrap_or_else(|| "userspace daemon is not running".into()),
            Err(error) => error.to_string(),
        };
        if tokio::time::Instant::now() >= deadline {
            return Err(TailscaleError::DaemonUnavailable(message));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_until_reachable(cli: &TailscaleCli) -> Result<TailscaleStatus, TailscaleError> {
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
    loop {
        match cli.status().await {
            Ok(status) => return Ok(status),
            Err(error) if is_retryable_startup_error(&error) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(TailscaleError::DaemonUnavailable(error.to_string()));
                }
            }
            Err(error) => return Err(error),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn is_retryable_startup_error(error: &TailscaleError) -> bool {
    matches!(
        error,
        TailscaleError::DaemonUnavailable(_) | TailscaleError::InvalidJson
    )
}

fn is_running(status: &TailscaleStatus) -> bool {
    status
        .backend_state
        .as_deref()
        .is_some_and(|state| state.eq_ignore_ascii_case("running"))
        || status
            .self_node
            .as_ref()
            .is_some_and(|node| node.online.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{TailscaleConfig, TailscaleMode};

    #[test]
    fn daemon_path_tracks_windows_cli_name() {
        assert_eq!(
            daemon_executable(Path::new(r"C:\Program Files\Tailscale\tailscale.exe")),
            PathBuf::from(r"C:\Program Files\Tailscale\tailscaled.exe")
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_path_preserves_unix_install_directory() {
        assert_eq!(
            daemon_executable(Path::new("/usr/local/bin/tailscale")),
            PathBuf::from("/usr/local/bin/tailscaled")
        );
        assert_eq!(
            daemon_executable(Path::new("tailscale")),
            PathBuf::from("tailscaled")
        );
    }

    #[test]
    fn relative_embedded_state_is_rejected() {
        let runtime = TailscaleRuntime::new("tailscale", std::env::temp_dir());
        let config = TailscaleConfig {
            enabled: true,
            mode: TailscaleMode::EmbeddedUserspace,
            device_name: None,
            auth_key_ref: None,
            state_dir: Some("relative/state".into()),
        };
        assert_eq!(
            runtime.state_path(&config),
            Err(TailscaleError::InvalidStatePath)
        );
    }

    #[test]
    fn incomplete_startup_status_is_retryable() {
        assert!(is_retryable_startup_error(&TailscaleError::InvalidJson));
        assert!(is_retryable_startup_error(
            &TailscaleError::DaemonUnavailable("pipe is starting".into())
        ));
        assert!(!is_retryable_startup_error(
            &TailscaleError::CommandFailed {
                exit_code: 2,
                stderr: "bad flag".into(),
            }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn private_state_cannot_be_owned_by_two_process_runtimes() {
        let unique = format!(
            "kodework-tailscale-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| unreachable!("create test directory: {error}"));
        let state = directory.join("tailscale.state");
        let first = acquire_state_lock(&state)
            .unwrap_or_else(|error| unreachable!("first lock succeeds: {error}"));
        assert!(matches!(
            acquire_state_lock(&state),
            Err(TailscaleError::DaemonUnavailable(_))
        ));
        drop(first);
        let second = acquire_state_lock(&state)
            .unwrap_or_else(|error| unreachable!("lock is released on drop: {error}"));
        drop(second);
        let _ = std::fs::remove_file(state.with_extension("lock"));
        let _ = std::fs::remove_dir(directory);
    }

    #[tokio::test]
    async fn embedded_status_rejects_a_different_active_state() {
        let root =
            std::env::temp_dir().join(format!("kodework-tailscale-status-{}", std::process::id()));
        let runtime = TailscaleRuntime::new("tailscale", &root);
        let active_state = root.join("other").join("tailscale.state");
        let requested_state = root.join("requested").join("tailscale.state");
        {
            let mut inner = runtime.inner.lock().await;
            inner.socket = Some(root.join("socket"));
            inner.state_path = Some(active_state);
        }
        let config = TailscaleConfig {
            enabled: true,
            mode: TailscaleMode::EmbeddedUserspace,
            device_name: None,
            auth_key_ref: None,
            state_dir: Some(requested_state.to_string_lossy().into_owned()),
        };
        let result = runtime.status_for_config(Some(&config)).await;
        let _ = std::fs::remove_dir_all(root);
        assert!(matches!(
            result,
            Err(TailscaleError::DaemonUnavailable(message))
                if message.contains("another embedded Tailscale state")
        ));
    }
}
