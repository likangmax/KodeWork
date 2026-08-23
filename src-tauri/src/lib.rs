#![forbid(unsafe_code)]

use kodework_core::session::SessionManager;
use kodework_network::{CandidateResolver, ResolverPolicy};
use kodework_ssh::host_key::HostKeyBroker;
use kodework_ssh::keyboard_interactive::KeyboardInteractiveBroker;
use kodework_storage::Database;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;
use thiserror::Error;

mod commands;
mod host_keys;
mod secrets;

pub struct AppState {
    pub(crate) database: Arc<Mutex<Database>>,
    /// Host-key decision broker backed by SQLite persistence.
    pub(crate) host_key: Arc<HostKeyBroker>,
    pub(crate) keyboard_interactive: Arc<KeyboardInteractiveBroker>,
    /// Connection/session manager (generation-guarded).
    pub(crate) sessions: SessionManager,
    /// Native OS credential adapter. Secret bytes never enter SQLite or
    /// renderer state and are only materialized for an in-flight connect.
    pub(crate) secrets: Arc<Mutex<secrets::Store>>,
    /// Optional managed userspace Tailscale lifecycle. System-daemon mode is
    /// observed through the same controller but never modified.
    pub(crate) tailscale: Arc<kodework_tailscale::runtime::TailscaleRuntime>,
    /// Local ConPTY sessions (PowerShell, CMD and WSL), deliberately separate
    /// from remote SSH lifecycle and reconnect state.
    pub(crate) local_terminals: kodework_local_pty::LocalTerminalManager,
    /// Host ids currently owned by the native reconnect supervisor.
    pub(crate) reconnecting: Arc<Mutex<HashSet<kodework_domain::HostId>>>,
    /// Host ids with a native reconnect profile. Credential bytes are never
    /// cached here; the supervisor resolves managed secrets on each attempt.
    pub(crate) reconnect_profiles: Arc<Mutex<HashSet<kodework_domain::HostId>>>,
    /// Host ids currently undergoing run reconciliation. UI refreshes are
    /// single-flight per host so tab changes cannot duplicate SSH probes.
    pub(crate) reconciling: Arc<Mutex<HashSet<kodework_domain::HostId>>>,
    /// Monotonic wake signal used to interrupt reconnect backoff after OS
    /// resume or explicit user focus without retaining credential bytes.
    pub(crate) reconnect_wake_epoch: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] kodework_storage::StorageError),
    #[error("invalid host: {0}")]
    InvalidHost(#[from] kodework_domain::DomainError),
    #[error("application data directory is unavailable")]
    MissingDataDirectory,
    #[error("application state lock is poisoned")]
    StatePoisoned,
    #[error("desktop runtime error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("{0}")]
    Session(String),
}

impl AppState {
    pub fn open() -> Result<Self, AppError> {
        let directory = data_directory().ok_or(AppError::MissingDataDirectory)?;
        std::fs::create_dir_all(&directory).map_err(|_| AppError::MissingDataDirectory)?;
        let database = Arc::new(Mutex::new(Database::open(
            directory.join("kodework.sqlite3"),
        )?));
        {
            let db = database.lock().map_err(|_| AppError::StatePoisoned)?;
            kodework_storage::repositories::RunRepository::new(db.connection())
                .recover_orphaned_quick_runs()?;
        }

        // Host-key persistence is a metadata table; the broker owns the
        // user decision flow (trust once / trust and save / reject).
        let known_hosts = Arc::new(host_keys::SqliteKnownHosts::new(Arc::clone(&database)));
        let host_key = Arc::new(HostKeyBroker::new(known_hosts, Duration::from_secs(60)));
        let keyboard_interactive =
            Arc::new(KeyboardInteractiveBroker::new(Duration::from_secs(120)));

        // Address discovery: use a managed controller so EmbeddedUserspace
        // and the system daemon cannot accidentally share a control socket.
        let tailscale_executable = resolve_tailscale_executable();
        let tailscale = Arc::new(kodework_tailscale::runtime::TailscaleRuntime::new(
            tailscale_executable,
            directory.join("tailscale"),
        ));
        let resolver = CandidateResolver::new(
            vec![Arc::new(
                kodework_tailscale::provider::TailscaleAddressProvider::from_runtime(Arc::clone(
                    &tailscale,
                )),
            )],
            ResolverPolicy::default(),
        );

        let sessions = SessionManager::new(Arc::clone(&host_key), resolver, 512);
        let secrets = Arc::new(Mutex::new(secrets::new_store()));
        let local_terminals = kodework_local_pty::LocalTerminalManager::new();

        Ok(Self {
            database,
            host_key,
            keyboard_interactive,
            sessions,
            secrets,
            tailscale,
            local_terminals,
            reconnecting: Arc::new(Mutex::new(HashSet::new())),
            reconnect_profiles: Arc::new(Mutex::new(HashSet::new())),
            reconciling: Arc::new(Mutex::new(HashSet::new())),
            reconnect_wake_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }
}

fn resolve_tailscale_executable() -> PathBuf {
    let mut candidates = Vec::new();
    let executable_suffix = std::env::consts::EXE_SUFFIX;
    let cli_name = format!("tailscale{executable_suffix}");
    let sidecar_name = format!("tailscale-{TARGET_TRIPLE}{executable_suffix}");
    // Tauri installs external binaries beside the main executable with the
    // target suffix removed. Prefer that audited, pinned build so embedded
    // mode works on a clean installation on every supported desktop OS.
    if let Ok(current_executable) = std::env::current_exe() {
        if let Some(directory) = current_executable.parent() {
            candidates.push(directory.join(&cli_name));
        }
    }
    // Development builds do not necessarily copy sidecars into target/debug.
    // Scan the target-specific sidecar directory instead of embedding a
    // Windows triple in shared code; Tauri names external binaries with the
    // target triple as a suffix.
    let sidecar_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    if let Ok(entries) = std::fs::read_dir(&sidecar_directory) {
        let mut sidecars = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case(&sidecar_name))
            })
            .collect::<Vec<_>>();
        sidecars.sort();
        candidates.extend(sidecars);
    }
    #[cfg(windows)]
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(program_files).join(r"Tailscale\tailscale.exe"));
    }
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local_app_data).join(r"Tailscale\tailscale.exe"));
    }
    #[cfg(unix)]
    {
        candidates.extend([
            PathBuf::from("/usr/local/bin/tailscale"),
            PathBuf::from("/usr/bin/tailscale"),
        ]);
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("tailscale"))
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
const TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(all(target_os = "windows", target_arch = "aarch64", target_env = "msvc"))]
const TARGET_TRIPLE: &str = "aarch64-pc-windows-msvc";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"))]
const TARGET_TRIPLE: &str = "aarch64-unknown-linux-gnu";
#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
    all(target_os = "windows", target_arch = "aarch64", target_env = "msvc"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
    all(target_os = "linux", target_arch = "aarch64", target_env = "gnu")
)))]
const TARGET_TRIPLE: &str = "unsupported-target";

pub fn run() -> Result<(), AppError> {
    let state = AppState::open()?;
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus the existing window instead of starting a second instance.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            commands::wake_connection_supervisor(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            setup_tray(app)?;
            commands::start_connection_supervisor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close hides to the tray; only tray Quit exits the process.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            } else if matches!(event, tauri::WindowEvent::Focused(true)) {
                commands::wake_connection_supervisor(window.app_handle());
            }
        })
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::list_hosts,
            commands::save_host,
            commands::save_host_password,
            commands::delete_host,
            commands::connect_host,
            commands::prepare_host_network,
            commands::disconnect_host,
            commands::session_state,
            commands::session_runtime_subscribe,
            commands::open_pane,
            commands::close_pane,
            commands::send_input,
            commands::resize_pty,
            commands::pending_host_key_requests,
            commands::answer_host_key,
            commands::pending_keyboard_interactive_requests,
            commands::answer_keyboard_interactive,
            commands::session_subscribe,
            commands::set_autostart,
            commands::autostart_status,
            commands::tmux_list,
            commands::tmux_new,
            commands::tmux_kill,
            commands::herdr_detect,
            commands::herdr_agents,
            commands::herdr_attach,
            commands::sftp_list,
            commands::sftp_upload,
            commands::clipboard_paste,
            commands::clipboard_copy_text,
            commands::sftp_download,
            commands::sftp_pause,
            commands::sftp_resume,
            commands::sftp_cancel,
            commands::sftp_subscribe,
            commands::sftp_dropped_events,
            commands::tunnel_open,
            commands::tunnel_close,
            commands::tunnel_list,
            commands::herdr_bridge,
            commands::herdr_bridge_stop_by_id,
            commands::snippet_list,
            commands::snippet_save,
            commands::snippet_delete,
            commands::yazi_available,
            commands::yazi_attach,
            commands::project_list,
            commands::project_save,
            commands::project_delete,
            commands::action_list,
            commands::action_save,
            commands::action_delete,
            commands::run_action,
            commands::run_list,
            commands::run_reconcile,
            commands::tailscale_status,
            commands::tailscale_runtime_info,
            commands::save_tailscale_auth_key,
            commands::local_terminal_capabilities,
            commands::local_terminal_open,
            commands::local_terminal_subscribe,
            commands::local_terminal_write,
            commands::local_terminal_resize,
            commands::local_terminal_close,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Resumed) {
                commands::wake_connection_supervisor(app);
            }
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    state.local_terminals.shutdown();
                    tauri::async_runtime::block_on(state.tailscale.shutdown());
                }
            }
        });
    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItem::with_id(app, "open", "打开 Kodework", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default icon".into()))?;
    TrayIconBuilder::with_id("kodework-tray")
        .icon(icon)
        .tooltip("Kodework")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                crate::commands::wake_connection_supervisor(app);
            }
            "quit" => {
                // Explicit quit: stop locally owned userspace Tailscale before
                // asking Tauri to terminate. Tauri's direct exit path performs
                // process termination, so relying only on RunEvent::Exit can
                // leave the embedded daemon behind and make the next launch
                // pay stale-PID cleanup latency.
                if let Some(state) = app.try_state::<AppState>() {
                    state.local_terminals.shutdown();
                    tauri::async_runtime::block_on(state.tailscale.shutdown());
                }
                app.cleanup_before_exit();
                std::process::exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn data_directory() -> Option<PathBuf> {
    dirs::data_local_dir().map(|path| path.join("Kodework"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_directory_is_app_scoped() {
        assert!(data_directory().is_some_and(|path| path.ends_with("Kodework")));
    }

    #[test]
    fn desktop_capabilities_cover_only_the_renderer_plugins_in_use() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .unwrap_or_else(|error| unreachable!("capability JSON: {error}"));
        let permissions = capability["permissions"]
            .as_array()
            .unwrap_or_else(|| unreachable!("permissions array"));
        let values = permissions
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            vec!["core:default", "dialog:default", "updater:default"]
        );
    }

    #[test]
    fn production_csp_allows_only_loopback_web_previews() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .unwrap_or_else(|error| unreachable!("tauri config JSON: {error}"));
        let csp = config["app"]["security"]["csp"]
            .as_str()
            .unwrap_or_else(|| unreachable!("CSP string"));
        assert!(csp.contains("frame-src http://127.0.0.1:* http://localhost:*"));
        assert!(csp.contains("object-src 'none'"));
        assert!(!csp.contains("frame-src *"));
    }
}
