#![forbid(unsafe_code)]

use crate::{AppError, AppState};
use kodework_domain::{
    classify_danger, validate_action, validate_host, validate_project, AuthenticationMode,
    ConnectionState, Host, HostId, TailscaleConfig, TailscaleMode,
};
use kodework_secrets::SecretStore;
use kodework_ssh::connection::AuthMethod;
use kodework_ssh::host_key::{HostKeyDecision, HostKeyRequest};
use tauri::State;

#[tauri::command]
pub(crate) fn local_terminal_capabilities() -> kodework_local_pty::LocalTerminalCapabilities {
    kodework_local_pty::LocalTerminalManager::capabilities()
}

#[tauri::command]
pub(crate) fn local_terminal_open(
    state: State<'_, AppState>,
    kind: kodework_local_pty::LocalTerminalKind,
    distribution: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<kodework_local_pty::LocalTerminalDescriptor, String> {
    state
        .local_terminals
        .open(kind, distribution, cols, rows)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn local_terminal_subscribe(
    state: State<'_, AppState>,
    id: u32,
    on_event: tauri::ipc::Channel<kodework_local_pty::LocalTerminalEvent>,
) -> Result<(), String> {
    let mut events = state
        .local_terminals
        .subscribe(id)
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn local_terminal_write(
    state: State<'_, AppState>,
    id: u32,
    data: Vec<u8>,
) -> Result<(), String> {
    state
        .local_terminals
        .write(id, &data)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn local_terminal_resize(
    state: State<'_, AppState>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .local_terminals
        .resize(id, cols, rows)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn local_terminal_close(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    state
        .local_terminals
        .close(id)
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ClipboardPasteResult {
    Text { text: String },
    Assets { remote_paths: Vec<String> },
    Empty,
}

#[derive(serde::Serialize)]
pub(crate) struct TailscaleRuntimeInfo {
    cli_available: bool,
    daemon_available: bool,
    bundled: bool,
    bundled_version: &'static str,
}

#[tauri::command]
pub(crate) fn list_hosts(state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .list_hosts()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn save_host(state: State<'_, AppState>, mut host: Host) -> Result<(), String> {
    // Renderer state is editable data, not an authority for credential
    // references. Preserve the server-owned SSH credential and Tailscale
    // auth-key reference so a stale/tampered IPC payload cannot point this
    // host at another host's secret. The dedicated credential commands remain
    // the only way to create or replace those references.
    let existing = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host.id)
        .map_err(|error| error.to_string())?;
    let (previous_auth_ref, previous_tailscale_ref) = if let Some(existing) = existing {
        let previous_auth_ref = existing.auth_ref.clone();
        // A credential has meaning only inside its authentication mode. Never
        // reinterpret a saved password as a private-key passphrase (or vice
        // versa) after the renderer changes the mode.
        host.auth_ref = if existing.auth_mode == host.auth_mode {
            existing.auth_ref
        } else {
            None
        };
        let previous = existing
            .tailscale
            .as_ref()
            .and_then(|config| config.auth_key_ref.clone());
        if let Some(config) = host.tailscale.as_mut() {
            config.auth_key_ref = previous.clone();
        }
        (previous_auth_ref, previous)
    } else {
        // New hosts cannot import credential references supplied by the
        // renderer; those must be created by the dedicated commands.
        host.auth_ref = None;
        if let Some(config) = host.tailscale.as_mut() {
            config.auth_key_ref = None;
        }
        (None, None)
    };
    validate_host(&host).map_err(|error| AppError::InvalidHost(error).to_string())?;
    state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .upsert_host(&host)
        .map_err(|error| error.to_string())?;

    if previous_auth_ref.as_ref() != host.auth_ref.as_ref() {
        if let Some(reference) = previous_auth_ref
            .as_ref()
            .filter(|reference| crate::secrets::is_managed_reference(reference))
        {
            if let Ok(mut secrets) = state.secrets.lock() {
                let _ = secrets.delete(reference);
            }
        }
    }

    // If the user explicitly removed the Tailscale section, delete the old
    // Windows credential after the database commit. This ordering keeps the
    // persisted host authoritative if the database write fails.
    let current_tailscale_ref = host
        .tailscale
        .as_ref()
        .and_then(|config| config.auth_key_ref.as_ref());
    if previous_tailscale_ref.as_ref() != current_tailscale_ref {
        if let Some(reference) = previous_tailscale_ref
            .as_ref()
            .filter(|reference| crate::secrets::is_managed_reference(reference))
        {
            if let Ok(mut secrets) = state.secrets.lock() {
                let _ = secrets.delete(reference);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_host(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<bool, String> {
    // Deleting a host must not leave an SSH transport, tunnel or PTY alive
    // after its persistent record and credential references are gone.
    if let Ok(mut active) = state.reconnecting.lock() {
        active.remove(&host_id);
    }
    let _ = state.sessions.disconnect(host_id).await;
    let (auth_ref, tailscale_auth_ref) = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host_id)
        .map_err(|error| error.to_string())?
        .map(|host| {
            (
                host.auth_ref,
                host.tailscale.and_then(|config| config.auth_key_ref),
            )
        })
        .unwrap_or((None, None));
    let deleted = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .delete_host(host_id)
        .map_err(|error| error.to_string())?;
    if deleted {
        if let Some(reference) = auth_ref
            .as_ref()
            .filter(|reference| crate::secrets::is_managed_reference(reference))
        {
            // The database deletion is authoritative. Credential cleanup is
            // best-effort because the store can be unavailable during logoff.
            let _ = state
                .secrets
                .lock()
                .map_err(|_| AppError::StatePoisoned.to_string())?
                .delete(reference);
        }
        if let Some(reference) = tailscale_auth_ref
            .as_ref()
            .filter(|reference| crate::secrets::is_managed_reference(reference))
        {
            let _ = state
                .secrets
                .lock()
                .map_err(|_| AppError::StatePoisoned.to_string())?
                .delete(reference);
        }
    }
    Ok(deleted)
}

/// Stores a host password in the native OS credential store and writes only its
/// opaque reference to SQLite. If the database update fails, the secret write
/// is rolled back so no orphan is created.
#[tauri::command]
pub(crate) fn save_host_password(
    state: State<'_, AppState>,
    host: Host,
    password: String,
) -> Result<Host, String> {
    use kodework_domain::CredentialRef;
    use zeroize::Zeroizing;

    // Only the password is supplied by this command. Reload the host so an
    // IPC caller cannot combine a valid host id with altered endpoints or
    // Tailscale references while storing a credential.
    let mut host = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "host does not exist".to_string())?;
    validate_host(&host).map_err(|error| AppError::InvalidHost(error).to_string())?;
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err("password must not be empty".to_string());
    }
    let previous_reference = host.auth_ref.clone();
    let credential_kind = match host.auth_mode {
        AuthenticationMode::Password => "ssh-password",
        AuthenticationMode::PublicKey => "ssh-key-passphrase",
        AuthenticationMode::SshAgent | AuthenticationMode::KeyboardInteractive => {
            return Err("selected authentication mode does not accept a saved password".into());
        }
    };
    let reference = CredentialRef {
        provider: crate::secrets::provider(),
        opaque_id: format!("{credential_kind}/{}", host.id.as_uuid()),
    };
    // The reference is deterministic, so replacing a credential can overwrite
    // the only copy of the old value. Keep the previous bytes until the DB
    // transaction succeeds, then restore them if SQLite rejects the update.
    let previous_secret = if previous_reference.as_ref() == Some(&reference) {
        state
            .secrets
            .lock()
            .ok()
            .and_then(|store| store.get(&reference).ok())
            .map(|secret| Zeroizing::new(secret.expose().to_vec()))
    } else {
        None
    };
    {
        let mut secrets = state
            .secrets
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        secrets
            .put(reference.clone(), password.as_bytes())
            .map_err(|error| format!("credential write failed: {error}"))?;
    }
    host.auth_ref = Some(reference.clone());
    let stored = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .upsert_host(&host);
    if let Err(error) = stored {
        if let Ok(mut secrets) = state.secrets.lock() {
            if let Some(previous_secret) = previous_secret {
                let _ = secrets.put(reference.clone(), previous_secret.as_slice());
            } else {
                let _ = secrets.delete(&reference);
            }
        }
        return Err(error.to_string());
    }
    if previous_reference.as_ref() != Some(&reference) {
        if let Some(previous) = previous_reference
            .as_ref()
            .filter(|value| crate::secrets::is_managed_reference(value))
        {
            if let Ok(mut secrets) = state.secrets.lock() {
                let _ = secrets.delete(previous);
            }
        }
    }
    Ok(host)
}

/// Connects to a host. Passwords are a one-shot in-memory credential:
/// the renderer sends it once over IPC for this connection only, the
/// Rust side zeroizes it after use, and it is never persisted to disk,
/// SQLite, logs or React state beyond the in-flight dialog.
#[tauri::command]
pub(crate) async fn connect_host(
    state: State<'_, AppState>,
    host: Host,
    password: Option<String>,
) -> Result<String, String> {
    connect_host_inner(&state, host, password).await
}

async fn connect_host_inner(
    state: &AppState,
    host: Host,
    password: Option<String>,
) -> Result<String, String> {
    // The renderer may hold a stale or tampered copy of a Host.  Resolve the
    // authoritative record by id before selecting credentials or addresses;
    // otherwise an IPC caller could combine one host's credential reference
    // with another host's endpoint.
    let host = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "host does not exist".to_string())?;
    validate_host(&host).map_err(|error| AppError::InvalidHost(error).to_string())?;
    // Embedded userspace mode is the only mode Kodework starts itself. A
    // missing/invalid auth key is reported before SSH address resolution so
    // the user sees a Tailscale error rather than an opaque network timeout.
    let _tailscale_key = if let Some(config) = host
        .tailscale
        .as_ref()
        .filter(|config| config.enabled && config.mode == TailscaleMode::EmbeddedUserspace)
    {
        let key = config
            .auth_key_ref
            .as_ref()
            .filter(|reference| crate::secrets::is_managed_reference(reference))
            .map(|reference| {
                state
                    .secrets
                    .lock()
                    .map_err(|_| AppError::StatePoisoned.to_string())?
                    .get(reference)
                    .map(|secret| zeroize::Zeroizing::new(secret.expose().to_vec()))
                    .map_err(|error| format!("Tailscale auth key lookup failed: {error}"))
            })
            .transpose()?;
        state
            .tailscale
            .ensure(config, key.as_ref().map(|value| value.as_slice()))
            .await
            .map_err(|error| format!("Tailscale userspace startup failed: {error}"))?;
        key
    } else {
        None
    };
    let supplied_secret = password
        .filter(|value| !value.is_empty())
        .map(|value| kodework_ssh::connection::ZeroizingVec::new(value.into_bytes()));
    let stored_secret = if supplied_secret.is_none() {
        match host.auth_ref.as_ref() {
            Some(reference) if crate::secrets::is_managed_reference(reference) => {
                let secret = state
                    .secrets
                    .lock()
                    .map_err(|_| AppError::StatePoisoned.to_string())?
                    .get(reference)
                    .map_err(|error| format!("credential lookup failed: {error}"))?;
                if secret.expose().is_empty() {
                    return Err("stored credential is empty".to_string());
                }
                Some(kodework_ssh::connection::ZeroizingVec::new(
                    secret.expose().to_vec(),
                ))
            }
            Some(reference) => {
                return Err(format!(
                    "credential provider {:?} is not supported for SSH authentication",
                    reference.provider
                ));
            }
            None => None,
        }
    } else {
        None
    };
    let secret = supplied_secret.or(stored_secret);
    let auth = match host.auth_mode {
        AuthenticationMode::Password => vec![AuthMethod::Password(
            secret.ok_or_else(|| "password is required".to_string())?,
        )],
        AuthenticationMode::PublicKey => {
            let key_path = host
                .private_key_path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
                .map(expand_user_path)
                .unwrap_or_else(|| {
                    dirs::home_dir()
                        .map(|home| home.join(".ssh").join("id_ed25519"))
                        .unwrap_or_else(|| std::path::PathBuf::from(".ssh/id_ed25519"))
                });
            if !key_path.is_file() {
                return Err(format!(
                    "private key does not exist: {}",
                    key_path.display()
                ));
            }
            vec![AuthMethod::PublicKey {
                key_path,
                passphrase: secret,
            }]
        }
        AuthenticationMode::SshAgent => vec![AuthMethod::SshAgent],
        AuthenticationMode::KeyboardInteractive => vec![AuthMethod::KeyboardInteractive {
            broker: std::sync::Arc::clone(&state.keyboard_interactive),
        }],
    };
    let outcome = state.sessions.connect(&host, auth).await?;
    match outcome {
        kodework_core::session::SessionOutcome::Connected { generation, .. } => {
            Ok(format!("connected; generation {generation}"))
        }
        kodework_core::session::SessionOutcome::Failed { reason, .. } => Err(reason),
    }
}

/// Runs the bounded reconnect policy in the native layer. The renderer only
/// requests supervision; credential lookup, backoff and single-flight
/// ownership stay inside the desktop process.
#[tauri::command]
pub(crate) async fn reconnect_host(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<String, String> {
    if state.sessions.state(host_id) != ConnectionState::Reconnecting {
        return Err("session is not waiting for reconnect".to_string());
    }
    let host = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "host does not exist".to_string())?;
    let automatic = host.auth_ref.is_some()
        || matches!(
            host.auth_mode,
            AuthenticationMode::PublicKey
                | AuthenticationMode::SshAgent
                | AuthenticationMode::KeyboardInteractive
        );
    if !automatic {
        return Err("interactive credentials are required".to_string());
    }
    {
        let mut active = state
            .reconnecting
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        if !active.insert(host_id) {
            return Ok("reconnect already in progress".to_string());
        }
    }
    let result = async {
        let mut last_error = String::from("reconnect failed");
        for attempt in 0..3u32 {
            // `connect_host_inner` legitimately moves the session through
            // Resolving/Connecting and leaves it Failed after a transient
            // attempt. The reconnect ownership set, not the transient
            // connection state, is the retry gate; otherwise the first
            // failure cancels attempts 2 and 3.
            if !reconnect_is_active(&state, host_id)? {
                return Err("reconnect cancelled".to_string());
            }
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(1200 * attempt as u64)).await;
                if !reconnect_is_active(&state, host_id)? {
                    return Err("reconnect cancelled".to_string());
                }
            }
            match connect_host_inner(&state, host.clone(), None).await {
                Ok(message) => {
                    if state.sessions.state(host_id) == ConnectionState::Ready {
                        return Ok(message);
                    }
                    return Err("reconnect cancelled".to_string());
                }
                Err(error) => {
                    last_error = error.clone();
                    if !reconnect_error_is_retryable(&error) {
                        break;
                    }
                }
            }
        }
        Err(last_error)
    }
    .await;
    state
        .reconnecting
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .remove(&host_id);
    result
}

fn reconnect_is_active(state: &AppState, host_id: HostId) -> Result<bool, String> {
    Ok(state
        .reconnecting
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .contains(&host_id))
}

fn reconnect_error_is_retryable(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    !lowered.starts_with("fatal connection error")
        && !lowered.contains("authentication failed")
        && !lowered.contains("permission denied")
        && !lowered.contains("private key")
        && !lowered.contains("invalid configuration")
        && !lowered.contains("key error")
        && !lowered.contains("encrypted")
        && !lowered.contains("decryption")
        && !lowered.contains("credential")
        && !lowered.contains("passphrase")
}

fn expand_user_path(value: &str) -> std::path::PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(trimmed)
}

/// Warms the optional managed network path before the user presses Connect.
/// This is deliberately separate from SSH authentication: it may start the
/// private userspace daemon and populate its short-lived status cache, but it
/// never opens a remote session. Concurrent calls are serialized by the
/// runtime lifecycle lock, so a connect racing the warm-up reuses the result.
#[tauri::command]
pub(crate) async fn prepare_host_network(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<(), String> {
    let host = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "host does not exist".to_string())?;
    let Some(config) = host
        .tailscale
        .as_ref()
        .filter(|config| config.enabled && config.mode == TailscaleMode::EmbeddedUserspace)
    else {
        return Ok(());
    };
    let key = config
        .auth_key_ref
        .as_ref()
        .filter(|reference| crate::secrets::is_managed_reference(reference))
        .map(|reference| {
            state
                .secrets
                .lock()
                .map_err(|_| AppError::StatePoisoned.to_string())?
                .get(reference)
                .map(|secret| zeroize::Zeroizing::new(secret.expose().to_vec()))
                .map_err(|error| format!("Tailscale auth key lookup failed: {error}"))
        })
        .transpose()?;
    state
        .tailscale
        .ensure(config, key.as_ref().map(|value| value.as_slice()))
        .await
        .map(|_| ())
        .map_err(|error| format!("Tailscale userspace startup failed: {error}"))
}

#[tauri::command]
pub(crate) async fn tailscale_status(
    state: State<'_, AppState>,
    host_id: Option<HostId>,
) -> Result<kodework_tailscale::TailscaleStatus, String> {
    let config = if let Some(host_id) = host_id {
        state
            .database
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?
            .get_host(host_id)
            .map_err(|error| error.to_string())?
            .and_then(|host| host.tailscale)
    } else {
        None
    };
    state
        .tailscale
        .status_for_config(config.as_ref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn tailscale_runtime_info(state: State<'_, AppState>) -> TailscaleRuntimeInfo {
    let (cli, daemon) = state.tailscale.component_paths();
    let same_as_app = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .zip(cli.parent().map(std::path::Path::to_path_buf))
        .is_some_and(|(app_dir, cli_dir)| app_dir == cli_dir);
    let dev_sidecar =
        cli.starts_with(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));
    let bundled = cli.is_file() && (same_as_app || dev_sidecar);
    TailscaleRuntimeInfo {
        cli_available: cli.is_file(),
        daemon_available: daemon.is_file(),
        bundled,
        bundled_version: "1.102.2",
    }
}

/// Stores a Tailscale auth key in the native OS credential store and associates
/// only its opaque reference with the selected host. The key is accepted once
/// over IPC and zeroized on return; it is never persisted in SQLite/logs.
#[tauri::command]
pub(crate) fn save_tailscale_auth_key(
    state: State<'_, AppState>,
    host_id: HostId,
    auth_key: String,
) -> Result<Host, String> {
    use kodework_domain::CredentialRef;
    use zeroize::Zeroizing;

    let auth_key = Zeroizing::new(auth_key);
    if auth_key.is_empty() || auth_key.chars().any(char::is_whitespace) {
        return Err("Tailscale auth key must be non-empty and contain no whitespace".into());
    }
    let mut host = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .get_host(host_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "host does not exist".to_string())?;
    let config = host.tailscale.get_or_insert(TailscaleConfig {
        enabled: true,
        mode: TailscaleMode::SystemDaemon,
        device_name: None,
        auth_key_ref: None,
        state_dir: None,
    });
    config.enabled = true;
    let reference = CredentialRef {
        provider: crate::secrets::provider(),
        opaque_id: format!("tailscale-auth/{}", host.id.as_uuid()),
    };
    let previous_reference = config
        .auth_key_ref
        .clone()
        .filter(crate::secrets::is_managed_reference);
    let previous_secret = config
        .auth_key_ref
        .as_ref()
        .filter(|old| crate::secrets::is_managed_reference(old))
        .and_then(|old| {
            state
                .secrets
                .lock()
                .ok()
                .and_then(|store| store.get(old).ok())
                .map(|secret| zeroize::Zeroizing::new(secret.expose().to_vec()))
        });
    state
        .secrets
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .put(reference.clone(), auth_key.as_bytes())
        .map_err(|error| format!("Tailscale auth key write failed: {error}"))?;
    config.auth_key_ref = Some(reference.clone());
    if let Err(error) = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?
        .upsert_host(&host)
    {
        if let Ok(mut secrets) = state.secrets.lock() {
            if let Some(previous_secret) = previous_secret {
                let _ = secrets.put(reference.clone(), previous_secret.as_slice());
            } else {
                let _ = secrets.delete(&reference);
            }
        }
        return Err(error.to_string());
    }
    if let Some(previous_reference) = previous_reference {
        if previous_reference != reference {
            if let Ok(mut secrets) = state.secrets.lock() {
                let _ = secrets.delete(&previous_reference);
            }
        }
    }
    Ok(host)
}

#[tauri::command]
pub(crate) async fn disconnect_host(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<(), String> {
    if let Ok(mut active) = state.reconnecting.lock() {
        active.remove(&host_id);
    }
    state.sessions.disconnect(host_id).await
}

#[tauri::command]
pub(crate) fn session_state(state: State<'_, AppState>, host_id: HostId) -> ConnectionState {
    state.sessions.state(host_id)
}

#[tauri::command]
pub(crate) async fn open_pane(
    state: State<'_, AppState>,
    host_id: HostId,
    cols: u32,
    rows: u32,
) -> Result<(u32, u32), String> {
    state.sessions.open_pane(host_id, cols, rows).await
}

#[tauri::command]
pub(crate) fn close_pane(
    state: State<'_, AppState>,
    host_id: HostId,
    pane_id: u32,
) -> Result<(), String> {
    state.sessions.close_pane(host_id, pane_id)
}

#[tauri::command]
pub(crate) async fn send_input(
    state: State<'_, AppState>,
    host_id: HostId,
    pane_id: u32,
    data: Vec<u8>,
) -> Result<(), String> {
    state.sessions.send_input(host_id, pane_id, &data).await
}

#[tauri::command]
pub(crate) async fn resize_pty(
    state: State<'_, AppState>,
    host_id: HostId,
    pane_id: u32,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    state.sessions.resize(host_id, pane_id, cols, rows).await
}

/// Pending host-key decisions for the renderer to display.
#[tauri::command]
pub(crate) fn pending_host_key_requests(state: State<'_, AppState>) -> Vec<HostKeyRequest> {
    state.host_key.drain_requests()
}

/// User decision for a pending host-key request.
#[tauri::command]
pub(crate) fn answer_host_key(
    state: State<'_, AppState>,
    request_id: u64,
    decision: String,
) -> bool {
    let decision = match decision.as_str() {
        "trust_once" => HostKeyDecision::TrustOnce,
        "trust_and_save" => HostKeyDecision::TrustAndSave,
        _ => HostKeyDecision::Reject,
    };
    state.host_key.answer(request_id, decision)
}

#[tauri::command]
pub(crate) fn pending_keyboard_interactive_requests(
    state: State<'_, AppState>,
) -> Result<Vec<kodework_ssh::keyboard_interactive::KeyboardInteractiveRequest>, String> {
    Ok(state.keyboard_interactive.drain_requests())
}

#[tauri::command]
pub(crate) fn answer_keyboard_interactive(
    state: State<'_, AppState>,
    request_id: u64,
    responses: Vec<String>,
) -> Result<bool, String> {
    if responses.len() > 32 || responses.iter().any(|response| response.len() > 4096) {
        return Err("keyboard-interactive response is too large".into());
    }
    Ok(state.keyboard_interactive.answer(request_id, responses))
}

/// Subscribes the renderer to terminal events for a host. The events flow
/// through a bounded Tauri Channel; the pump task dies with the session.
#[tauri::command]
pub(crate) async fn session_subscribe(
    state: State<'_, AppState>,
    host_id: HostId,
    channel: Option<u32>,
    on_event: tauri::ipc::Channel<kodework_ssh::handler::SessionEvent>,
) -> Result<(), String> {
    let mut events = state
        .sessions
        .subscribe(host_id, channel)
        .ok_or_else(|| "no session for host".to_string())?;
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// Enables or disables start-on-login (per-user startup entry).
#[tauri::command]
pub(crate) async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|error| error.to_string())?;
    } else {
        manager.disable().map_err(|error| error.to_string())?;
    }
    Ok(manager.is_enabled().unwrap_or(false))
}

/// Reports whether start-on-login is currently enabled.
#[tauri::command]
pub(crate) fn autostart_status(app: tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}
/// Lists remote tmux sessions.
#[tauri::command]
pub(crate) async fn tmux_list(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<Vec<kodework_core::session::TmuxSession>, String> {
    state.sessions.tmux_list(host_id).await
}

/// Creates a detached tmux session.
#[tauri::command]
pub(crate) async fn tmux_new(
    state: State<'_, AppState>,
    host_id: HostId,
    name: String,
) -> Result<(), String> {
    state.sessions.tmux_new(host_id, &name).await
}

/// Kills a remote tmux session (idempotent).
#[tauri::command]
pub(crate) async fn tmux_kill(
    state: State<'_, AppState>,
    host_id: HostId,
    name: String,
) -> Result<(), String> {
    state.sessions.tmux_kill(host_id, &name).await
}

/// Detects Herdr on the remote host; returns the version string.
#[tauri::command]
pub(crate) async fn herdr_detect(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<String, String> {
    let client = state.sessions.herdr_client(host_id);
    client.detect().await.map_err(|error| error.to_string())
}

/// Lists live Herdr agents.
#[tauri::command]
pub(crate) async fn herdr_agents(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<Vec<kodework_herdr::cli::HerdrAgentInfo>, String> {
    let client = state.sessions.herdr_client(host_id);
    client.agents().await.map_err(|error| error.to_string())
}

/// Focuses the PTY on the Herdr TUI.
#[tauri::command]
pub(crate) async fn herdr_attach(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<(), String> {
    state.sessions.herdr_attach(host_id).await
}
/// Lists a remote directory over SFTP.
#[tauri::command]
pub(crate) async fn sftp_list(
    state: State<'_, AppState>,
    host_id: HostId,
    path: String,
) -> Result<Vec<kodework_sftp::backend::RemoteFileMeta>, String> {
    state.sessions.sftp_list(host_id, &path).await
}

/// Enqueues an upload through the transfer manager.
#[tauri::command]
pub(crate) async fn sftp_upload(
    state: State<'_, AppState>,
    host_id: HostId,
    local_path: String,
    remote_path: String,
    resume: bool,
) -> Result<kodework_domain::TransferId, String> {
    state
        .sessions
        .sftp_upload(host_id, &local_path, &remote_path, resume)
        .await
}

/// Reads the native system clipboard. Images and PDFs are staged via
/// the existing atomic SFTP transfer manager, and their remote paths are
/// returned only after every upload completes successfully.
#[tauri::command]
pub(crate) async fn clipboard_paste(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<ClipboardPasteResult, String> {
    use kodework_platform::clipboard::{read_clipboard, ClipboardPayload};
    use std::path::Path;

    let temp_root = crate::data_directory()
        .ok_or_else(|| "应用数据目录不可用".to_string())?
        .join("clipboard");
    let payload = tauri::async_runtime::spawn_blocking(move || read_clipboard(&temp_root))
        .await
        .map_err(|error| format!("读取剪贴板任务失败：{error}"))??;
    match payload {
        ClipboardPayload::Text(text) => Ok(ClipboardPasteResult::Text { text }),
        ClipboardPayload::Empty => Ok(ClipboardPasteResult::Empty),
        ClipboardPayload::Assets(assets) => {
            let temporary_paths = assets
                .iter()
                .filter(|asset| asset.temporary)
                .map(|asset| asset.path.clone())
                .collect::<Vec<_>>();
            let mut remote_paths = Vec::with_capacity(assets.len());
            let upload_result = async {
                for asset in assets {
                    let transfer_id = kodework_domain::TransferId::new();
                    let remote_path = format!(
                        "/tmp/kodework-paste-{}.{}",
                        transfer_id.as_uuid(),
                        asset.extension
                    );
                    state
                        .sessions
                        .sftp_upload_and_wait(
                            host_id,
                            asset.path.to_string_lossy().as_ref(),
                            &remote_path,
                        )
                        .await?;
                    remote_paths.push(remote_path);
                }
                Ok::<(), String>(())
            }
            .await;
            for path in temporary_paths {
                if Path::new(&path).is_file() {
                    let _ = std::fs::remove_file(path);
                }
            }
            upload_result?;
            Ok(ClipboardPasteResult::Assets { remote_paths })
        }
    }
}

#[tauri::command]
pub(crate) async fn clipboard_copy_text(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || kodework_platform::clipboard::write_text(&text))
        .await
        .map_err(|error| format!("写入剪贴板任务失败：{error}"))?
}

/// Enqueues a download through the transfer manager.
#[tauri::command]
pub(crate) async fn sftp_download(
    state: State<'_, AppState>,
    host_id: HostId,
    remote_path: String,
    local_path: String,
    resume: bool,
) -> Result<kodework_domain::TransferId, String> {
    state
        .sessions
        .sftp_download(host_id, &remote_path, &local_path, resume)
        .await
}

#[tauri::command]
pub(crate) fn sftp_pause(
    state: State<'_, AppState>,
    host_id: HostId,
    transfer_id: kodework_domain::TransferId,
) -> Result<(), String> {
    state.sessions.sftp_pause(host_id, transfer_id)
}

#[tauri::command]
pub(crate) fn sftp_resume(
    state: State<'_, AppState>,
    host_id: HostId,
    transfer_id: kodework_domain::TransferId,
) -> Result<(), String> {
    state.sessions.sftp_resume(host_id, transfer_id)
}

#[tauri::command]
pub(crate) fn sftp_cancel(
    state: State<'_, AppState>,
    host_id: HostId,
    transfer_id: kodework_domain::TransferId,
) -> Result<(), String> {
    state.sessions.sftp_cancel(host_id, transfer_id)
}

/// Streams transfer events to the renderer. The manager is created on
/// first use so the subscription always has a live pump.
#[tauri::command]
pub(crate) async fn sftp_subscribe(
    state: State<'_, AppState>,
    host_id: HostId,
    on_event: tauri::ipc::Channel<kodework_sftp::manager::TransferEvent>,
) -> Result<(), String> {
    state.sessions.sftp_manager(host_id).await?;
    let Some(mut rx) = state.sessions.subscribe_transfers(host_id) else {
        return Err("no transfer manager for host".to_string());
    };
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn sftp_dropped_events(state: State<'_, AppState>, host_id: HostId) -> u64 {
    state.sessions.transfer_dropped_events(host_id)
}
/// Opens an SSH local port forward.
#[tauri::command]
pub(crate) async fn tunnel_open(
    state: State<'_, AppState>,
    host_id: HostId,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
) -> Result<kodework_core::tunnel::TunnelInfo, String> {
    state
        .sessions
        .open_tunnel(host_id, local_port, &remote_host, remote_port)
        .await
}

/// Closes a tunnel (idempotent).
#[tauri::command]
pub(crate) async fn tunnel_close(
    state: State<'_, AppState>,
    tunnel_id: kodework_domain::TunnelId,
) -> Result<(), String> {
    state.sessions.close_tunnel(tunnel_id).await
}

/// Lists all tunnels.
#[tauri::command]
pub(crate) fn tunnel_list(state: State<'_, AppState>) -> Vec<kodework_core::tunnel::TunnelInfo> {
    state.sessions.list_tunnels()
}
/// Bridges the remote herdr socket to a local loopback port.
#[tauri::command]
pub(crate) async fn herdr_bridge(
    state: State<'_, AppState>,
    host_id: HostId,
    local_port: u16,
) -> Result<kodework_core::session::HerdrBridgeInfo, String> {
    state.sessions.herdr_bridge(host_id, local_port).await
}

/// Stops the remote socat bridge.
#[tauri::command]
pub(crate) async fn herdr_bridge_stop(
    state: State<'_, AppState>,
    host_id: HostId,
    remote_port: u16,
    remote_pid: Option<u32>,
) -> Result<(), String> {
    state
        .sessions
        .herdr_bridge_stop(host_id, remote_port, remote_pid)
        .await
}
/// Lists all command snippets.
#[tauri::command]
pub(crate) fn snippet_list(
    state: State<'_, AppState>,
) -> Result<Vec<kodework_domain::Snippet>, String> {
    use kodework_storage::repositories::SnippetRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    SnippetRepository::new(db.connection())
        .list_all()
        .map_err(|error| error.to_string())
}

/// Creates or updates a snippet.
#[tauri::command]
pub(crate) fn snippet_save(
    state: State<'_, AppState>,
    snippet: kodework_domain::Snippet,
) -> Result<(), String> {
    use kodework_storage::repositories::SnippetRepository;
    if snippet.name.trim().is_empty() || snippet.text.is_empty() {
        return Err("片段名称与内容不能为空".to_string());
    }
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    SnippetRepository::new(db.connection())
        .upsert(&snippet)
        .map_err(|error| error.to_string())
}

/// Deletes a snippet.
#[tauri::command]
pub(crate) fn snippet_delete(
    state: State<'_, AppState>,
    snippet_id: kodework_domain::SnippetId,
) -> Result<bool, String> {
    use kodework_storage::repositories::SnippetRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    SnippetRepository::new(db.connection())
        .delete(snippet_id)
        .map_err(|error| error.to_string())
}
/// Detects remote yazi.
#[tauri::command]
pub(crate) async fn yazi_available(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<bool, String> {
    state.sessions.yazi_available(host_id).await
}

/// Launches yazi in pane 0.
#[tauri::command]
pub(crate) async fn yazi_attach(state: State<'_, AppState>, host_id: HostId) -> Result<(), String> {
    state.sessions.yazi_attach(host_id).await
}
// ---------- Workspace controls (projects / actions / runs) ----------

#[tauri::command]
pub(crate) fn project_list(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<Vec<kodework_domain::Project>, String> {
    use kodework_storage::repositories::ProjectRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    ProjectRepository::new(db.connection())
        .list_by_host(host_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn project_save(
    state: State<'_, AppState>,
    project: kodework_domain::Project,
) -> Result<(), String> {
    use kodework_storage::repositories::ProjectRepository;
    validate_project(&project).map_err(|error| error.to_string())?;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    let repository = ProjectRepository::new(db.connection());
    if db
        .get_host(project.host_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("project host does not exist".to_string());
    }
    if let Some(existing) = repository
        .get(project.id)
        .map_err(|error| error.to_string())?
    {
        if existing.host_id != project.host_id {
            return Err("project cannot be moved to another host".to_string());
        }
    }
    repository
        .upsert(&project)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn project_delete(
    state: State<'_, AppState>,
    project_id: kodework_domain::ProjectId,
) -> Result<bool, String> {
    use kodework_storage::repositories::ProjectRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    ProjectRepository::new(db.connection())
        .delete(project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn action_list(
    state: State<'_, AppState>,
    project_id: kodework_domain::ProjectId,
) -> Result<Vec<kodework_domain::Action>, String> {
    use kodework_storage::repositories::ActionRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    let mut actions = ActionRepository::new(db.connection())
        .list_by_project(project_id)
        .map_err(|error| error.to_string())?;
    // Older databases may contain a renderer-supplied danger label from
    // before server-side classification was enforced. Recompute it before
    // rendering so the confirmation dialog matches the backend decision.
    for action in &mut actions {
        action.danger_level = classify_danger(&action.command);
    }
    Ok(actions)
}

#[tauri::command]
pub(crate) fn action_save(
    state: State<'_, AppState>,
    mut action: kodework_domain::Action,
) -> Result<(), String> {
    use kodework_storage::repositories::ActionRepository;
    validate_action(&action).map_err(|error| error.to_string())?;
    if action.mode != kodework_domain::ActionMode::Quick {
        // Only Quick has a locally observable exec deadline. Do not retain a
        // stale timeout on Interactive/Background actions where it would
        // falsely suggest enforcement.
        action.timeout_ms = None;
    }
    // Danger classification is a server-side decision; never trust the
    // renderer-declared level (it gates the confirmation dialog).
    action.danger_level = classify_danger(&action.command);
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    let projects = kodework_storage::repositories::ProjectRepository::new(db.connection());
    if projects
        .get(action.project_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("action project does not exist".to_string());
    }
    let actions = ActionRepository::new(db.connection());
    if let Some(existing) = actions.get(action.id).map_err(|error| error.to_string())? {
        if existing.project_id != action.project_id {
            return Err("action cannot be moved to another project".to_string());
        }
    }
    actions.upsert(&action).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn action_delete(
    state: State<'_, AppState>,
    action_id: kodework_domain::ActionId,
) -> Result<bool, String> {
    use kodework_storage::repositories::ActionRepository;
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    ActionRepository::new(db.connection())
        .delete(action_id)
        .map_err(|error| error.to_string())
}

/// Runs an action. `confirmed` is checked server-side for dangerous
/// actions so the UI cannot bypass it.
#[tauri::command]
pub(crate) async fn run_action(
    state: State<'_, AppState>,
    host_id: HostId,
    action: kodework_domain::Action,
    confirmed: bool,
) -> Result<kodework_core::session::RunOutcome, String> {
    use kodework_domain::{Run, RunId, RunStatus};
    use kodework_storage::repositories::{ActionRepository, ProjectRepository, RunRepository};
    use std::time::{SystemTime, UNIX_EPOCH};

    // The database is authoritative. Never execute a renderer-supplied
    // command merely because it reuses the id of a saved action.
    let action = {
        let db = state
            .database
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        let stored = ActionRepository::new(db.connection())
            .get(action.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "action does not exist".to_string())?;
        let project = ProjectRepository::new(db.connection())
            .get(stored.project_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "action project does not exist".to_string())?;
        if project.host_id != host_id {
            return Err("action does not belong to the selected host".to_string());
        }
        stored
    };
    validate_action(&action).map_err(|error| error.to_string())?;
    if kodework_core::action_requires_confirmation(&action) && !confirmed {
        return Err("该动作需要确认后才能运行".to_string());
    }

    // An interactive command has no native exit boundary: the PTY shell owns
    // its eventual status and may run indefinitely. Persisting it as a
    // terminal Run would create a misleading permanent Unknown row, so only
    // Quick and Background actions enter Run History.
    if action.mode == kodework_domain::ActionMode::Interactive {
        return state
            .sessions
            .run_action_with_id(host_id, &action, confirmed, None)
            .await;
    }

    let started_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let run = Run {
        id: RunId::new(),
        action_id: Some(action.id),
        host_id,
        project_id: Some(action.project_id),
        action_name: action.name.clone(),
        command_snapshot: action.command.clone(),
        mode: action.mode,
        cwd_snapshot: action.cwd.clone(),
        status: RunStatus::Running,
        started_at_ms: Some(started_at_ms),
        finished_at_ms: None,
        exit_code: None,
        remote_session_ref: None,
        stdout_preview: String::new(),
        stderr_preview: String::new(),
        output_bytes: 0,
        last_reconciled_at_ms: None,
    };
    {
        let db = state
            .database
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        RunRepository::new(db.connection())
            .create(&run)
            .map_err(|error| error.to_string())?;
    }

    let outcome = state
        .sessions
        .run_action_with_id(host_id, &action, confirmed, Some(run.id))
        .await;
    let finished_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let (
        status,
        exit_code,
        finished_at_ms,
        remote_ref,
        output_bytes,
        stdout_preview,
        stderr_preview,
    ) = match &outcome {
        Ok(value) => match value.disposition {
            kodework_core::session::RunDisposition::BackgroundStarted => (
                RunStatus::Running,
                None,
                None,
                value.remote_session_ref.as_deref(),
                value.output_bytes,
                value.stdout_preview.as_str(),
                value.stderr_preview.as_str(),
            ),
            kodework_core::session::RunDisposition::Completed => {
                // A channel can close after the remote accepted the command
                // but before SSH delivered an exit status. That is not proof
                // of failure; keep the result explicitly unknowable so a
                // later reconciliation can provide authoritative evidence.
                let status = completed_run_status(value.exit_code);
                (
                    status,
                    value.exit_code,
                    (value.exit_code.is_some()).then_some(finished_at_ms),
                    value.remote_session_ref.as_deref(),
                    value.output_bytes,
                    value.stdout_preview.as_str(),
                    value.stderr_preview.as_str(),
                )
            }
            kodework_core::session::RunDisposition::InteractiveDispatched => (
                RunStatus::Unknown,
                None,
                Some(finished_at_ms),
                None,
                0,
                value.stdout_preview.as_str(),
                value.stderr_preview.as_str(),
            ),
        },
        Err(error)
            if action.mode == kodework_domain::ActionMode::Quick && is_run_timeout(error) =>
        {
            (
                RunStatus::TimedOut,
                None,
                Some(finished_at_ms),
                None,
                0,
                "",
                error.as_str(),
            )
        }
        Err(error) if action.mode == kodework_domain::ActionMode::Background => (
            // A transport/launcher error does not prove that the detached
            // tmux session was never created. Keep the row reconcilable so a
            // later connection can inspect the atomic remote marker.
            RunStatus::Unknown,
            None,
            None,
            None,
            0,
            "",
            error.as_str(),
        ),
        Err(error) => (
            RunStatus::Failed,
            None,
            Some(finished_at_ms),
            None,
            0,
            "",
            error.as_str(),
        ),
    };
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    RunRepository::new(db.connection())
        .finish(
            run.id,
            status,
            exit_code,
            finished_at_ms,
            remote_ref,
            output_bytes,
            stdout_preview,
            stderr_preview,
            None,
        )
        .map_err(|error| error.to_string())?;
    outcome
}

fn is_run_timeout(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("timed out") || lowered.contains("timeout")
}

fn completed_run_status(exit_code: Option<i32>) -> kodework_domain::RunStatus {
    match exit_code {
        Some(0) => kodework_domain::RunStatus::Succeeded,
        Some(_) => kodework_domain::RunStatus::Failed,
        None => kodework_domain::RunStatus::Unknown,
    }
}

/// Lists persisted Action runs for the Activity surface with a bounded page.
#[tauri::command]
pub(crate) fn run_list(
    state: State<'_, AppState>,
    host_id: Option<HostId>,
    action_id: Option<kodework_domain::ActionId>,
    limit: Option<u32>,
) -> Result<Vec<kodework_domain::Run>, String> {
    use kodework_storage::repositories::{ActionRepository, ProjectRepository, RunRepository};
    let limit = usize::try_from(limit.unwrap_or(50).clamp(1, 500)).unwrap_or(50);
    let db = state
        .database
        .lock()
        .map_err(|_| AppError::StatePoisoned.to_string())?;
    let repository = RunRepository::new(db.connection());
    match (host_id, action_id) {
        (Some(host_id), Some(id)) => {
            let action = ActionRepository::new(db.connection())
                .get(id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "action does not exist".to_string())?;
            let project = ProjectRepository::new(db.connection())
                .get(action.project_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "action project does not exist".to_string())?;
            if project.host_id != host_id {
                return Err("action does not belong to the selected host".to_string());
            }
            repository.list_by_action(id, limit)
        }
        (None, Some(id)) => repository.list_by_action(id, limit),
        (Some(host_id), None) => repository.list_recent_by_host(host_id, limit),
        (None, None) => repository.list_recent(limit),
    }
    .map_err(|error| error.to_string())
}

/// Reconcile persisted Quick/Background runs against the remote source of
/// truth. This is intentionally explicit and bounded so a reconnect cannot
/// spawn an unbounded number of SSH probes.
#[tauri::command]
pub(crate) async fn run_reconcile(
    state: State<'_, AppState>,
    host_id: HostId,
) -> Result<usize, String> {
    use kodework_storage::repositories::RunRepository;
    use std::time::{SystemTime, UNIX_EPOCH};

    let runs = {
        let db = state
            .database
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        RunRepository::new(db.connection())
            .list_reconcilable_by_host(host_id, 100)
            .map_err(|error| error.to_string())?
    };
    // Probe several detached runs concurrently, but keep a small bound so a
    // reconnect cannot turn a large history page into an SSH channel storm.
    let probe_limit = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut probes = Vec::with_capacity(runs.len());
    for run in runs {
        let permit = std::sync::Arc::clone(&probe_limit)
            .acquire_owned()
            .await
            .map_err(|_| "reconcile probe limit closed".to_string())?;
        let sessions = state.sessions.clone();
        probes.push(tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let result = sessions.reconcile_background_run(host_id, run.id).await;
            (run, result)
        }));
    }
    let mut completed_probes = Vec::with_capacity(probes.len());
    for probe in probes {
        match probe.await {
            Ok((run, Ok(remote))) => completed_probes.push((run, remote)),
            // A transport/probe failure is not evidence that the remote
            // command stopped. Persist Unknown for this row and continue
            // committing successful probes instead of making the batch
            // all-or-nothing.
            Ok((run, Err(_))) => {
                completed_probes.push((run, kodework_core::session::RemoteRunState::Unknown))
            }
            // A panicked/cancelled task cannot safely identify its run here.
            // Other completed probes are still authoritative and must be
            // committed; this row remains reconcilable on the next pass.
            Err(_) => {}
        }
    }

    let mut reconciled = 0usize;
    for (run, remote) in completed_probes {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64;
        let (status, exit_code, finished) = reconciled_run_fields(remote, run.started_at_ms, now);
        let db = state
            .database
            .lock()
            .map_err(|_| AppError::StatePoisoned.to_string())?;
        RunRepository::new(db.connection())
            .finish(
                run.id,
                status,
                exit_code,
                finished,
                run.remote_session_ref.as_deref(),
                run.output_bytes,
                &run.stdout_preview,
                &run.stderr_preview,
                Some(now),
            )
            .map_err(|error| error.to_string())?;
        reconciled += 1;
    }
    Ok(reconciled)
}

fn reconciled_run_fields(
    remote: kodework_core::session::RemoteRunState,
    local_started_at_ms: Option<u64>,
    observed_at_ms: u64,
) -> (kodework_domain::RunStatus, Option<i32>, Option<u64>) {
    use kodework_core::session::RemoteRunState;
    use kodework_domain::RunStatus;

    match remote {
        RemoteRunState::Running => (RunStatus::Running, None, None),
        RemoteRunState::Completed {
            exit_code,
            started_at_ms,
            finished_at_ms,
        } => (
            if exit_code == 0 {
                RunStatus::Succeeded
            } else {
                RunStatus::Failed
            },
            Some(exit_code),
            Some(match (local_started_at_ms, started_at_ms, finished_at_ms) {
                (Some(local_start), Some(remote_start), Some(remote_finish))
                    if remote_finish >= remote_start =>
                {
                    local_start.saturating_add(remote_finish.saturating_sub(remote_start))
                }
                // Without a local anchor, keep the UI timeline on the local
                // observation clock rather than mixing in a remote epoch.
                _ => observed_at_ms,
            }),
        ),
        // Unknown means that the remote source of truth is temporarily
        // unavailable; it remains reconcilable and must not look finished.
        RemoteRunState::Unknown => (RunStatus::Unknown, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        completed_run_status, is_run_timeout, reconciled_run_fields, reconnect_error_is_retryable,
    };
    use kodework_core::session::RemoteRunState;
    use kodework_domain::RunStatus;

    #[test]
    fn reconnect_does_not_retry_fatal_or_credential_errors() {
        assert!(!reconnect_error_is_retryable(
            "fatal connection error for host: host key changed"
        ));
        assert!(!reconnect_error_is_retryable("authentication failed"));
        assert!(!reconnect_error_is_retryable(
            "invalid configuration: key error: encrypted key"
        ));
        assert!(reconnect_error_is_retryable("connection timed out"));
        assert!(reconnect_error_is_retryable("remote host is unreachable"));
    }

    #[test]
    fn unknown_reconciliation_keeps_run_open_for_later_evidence() {
        assert_eq!(
            reconciled_run_fields(RemoteRunState::Unknown, Some(1_000), 123),
            (RunStatus::Unknown, None, None)
        );
        assert_eq!(
            reconciled_run_fields(
                RemoteRunState::Completed {
                    exit_code: 0,
                    started_at_ms: Some(1_000),
                    finished_at_ms: Some(2_000),
                },
                Some(1_000),
                123,
            ),
            (RunStatus::Succeeded, Some(0), Some(2_000))
        );
        assert_eq!(
            reconciled_run_fields(
                RemoteRunState::Completed {
                    exit_code: 17,
                    started_at_ms: None,
                    finished_at_ms: None,
                },
                Some(1_000),
                123,
            ),
            (RunStatus::Failed, Some(17), Some(123))
        );
    }

    #[test]
    fn reconciliation_anchors_remote_duration_to_local_start() {
        assert_eq!(
            reconciled_run_fields(
                RemoteRunState::Completed {
                    exit_code: 0,
                    started_at_ms: Some(5_000_000),
                    finished_at_ms: Some(5_010_000),
                },
                Some(1_000_000),
                9_999_999,
            ),
            (RunStatus::Succeeded, Some(0), Some(1_010_000))
        );
    }

    #[test]
    fn timeout_errors_are_classified_as_run_timeouts() {
        assert!(is_run_timeout("connection timed out"));
        assert!(is_run_timeout("remote command timeout"));
        assert!(!is_run_timeout("authentication failed"));
    }

    #[test]
    fn missing_exit_status_is_unknown_after_dispatch() {
        assert_eq!(completed_run_status(None), RunStatus::Unknown);
        assert_eq!(completed_run_status(Some(0)), RunStatus::Succeeded);
        assert_eq!(completed_run_status(Some(23)), RunStatus::Failed);
    }
}
