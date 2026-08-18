#![forbid(unsafe_code)]

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const MAX_SESSIONS: usize = 20;
const MAX_PENDING_EVENTS: usize = 128;
const EVENT_QUEUE: usize = 256;
const CHUNK_SIZE: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTerminalKind {
    PowerShell,
    CommandPrompt,
    Wsl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalTerminalDescriptor {
    pub id: u32,
    pub kind: LocalTerminalKind,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalTerminalEvent {
    Data { bytes: Vec<u8> },
    Exited { code: u32 },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalTerminalCapabilities {
    pub powershell: bool,
    pub command_prompt: bool,
    pub wsl: bool,
    pub wsl_distributions: Vec<String>,
}

#[derive(Debug, Error)]
pub enum LocalPtyError {
    #[error("local terminal limit reached (maximum {MAX_SESSIONS})")]
    LimitReached,
    #[error("local terminal {0} does not exist")]
    Missing(u32),
    #[error("local terminal lock is poisoned")]
    Poisoned,
    #[error("invalid WSL distribution: {0}")]
    InvalidDistribution(String),
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("I/O error: {0}")]
    Io(String),
}

struct LocalSession {
    descriptor: LocalTerminalDescriptor,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    subscriber: Mutex<Option<tokio::sync::mpsc::Sender<LocalTerminalEvent>>>,
    pending: Mutex<VecDeque<LocalTerminalEvent>>,
}

#[derive(Clone)]
pub struct LocalTerminalManager {
    sessions: Arc<Mutex<HashMap<u32, Arc<LocalSession>>>>,
    next_id: Arc<Mutex<u32>>,
}

impl Default for LocalTerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    pub fn capabilities() -> LocalTerminalCapabilities {
        let powershell = command_exists("pwsh.exe") || command_exists("powershell.exe");
        let command_prompt = command_exists("cmd.exe");
        let distributions = if command_exists("wsl.exe") {
            list_wsl_distributions()
        } else {
            Vec::new()
        };
        LocalTerminalCapabilities {
            powershell,
            command_prompt,
            wsl: !distributions.is_empty() || command_exists("wsl.exe"),
            wsl_distributions: distributions,
        }
    }

    pub fn open(
        &self,
        kind: LocalTerminalKind,
        distribution: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<LocalTerminalDescriptor, LocalPtyError> {
        let mut sessions = self.sessions.lock().map_err(|_| LocalPtyError::Poisoned)?;
        if sessions.len() >= MAX_SESSIONS {
            return Err(LocalPtyError::LimitReached);
        }
        let (program, args, label) = command_plan(&kind, distribution.as_deref())?;
        let id = {
            let mut next = self.next_id.lock().map_err(|_| LocalPtyError::Poisoned)?;
            let id = *next;
            *next = next.wrapping_add(1).max(1);
            id
        };
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: rows.clamp(2, 512),
                cols: cols.clamp(2, 512),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| LocalPtyError::Pty(e.to_string()))?;
        let mut command = CommandBuilder::new(program);
        command.args(args);
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|e| LocalPtyError::Pty(e.to_string()))?;
        // The parent must release its copy of the slave-side handles after
        // spawning. Keeping them open can prevent ConPTY from delivering
        // output/EOF and leaves both the reader and child waiting forever.
        drop(pty.slave);
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|e| LocalPtyError::Pty(e.to_string()))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|e| LocalPtyError::Pty(e.to_string()))?;
        let descriptor = LocalTerminalDescriptor { id, kind, label };
        let session = Arc::new(LocalSession {
            descriptor: descriptor.clone(),
            writer: Mutex::new(writer),
            master: Mutex::new(pty.master),
            child: Mutex::new(child),
            subscriber: Mutex::new(None),
            pending: Mutex::new(VecDeque::new()),
        });
        sessions.insert(id, Arc::clone(&session));
        drop(sessions);
        spawn_reader(Arc::clone(&session), reader);
        Ok(descriptor)
    }

    pub fn subscribe(
        &self,
        id: u32,
    ) -> Result<tokio::sync::mpsc::Receiver<LocalTerminalEvent>, LocalPtyError> {
        let session = self.get(id)?;
        let (sender, receiver) = tokio::sync::mpsc::channel(EVENT_QUEUE);
        {
            let mut subscriber = session
                .subscriber
                .lock()
                .map_err(|_| LocalPtyError::Poisoned)?;
            *subscriber = Some(sender.clone());
        }
        let mut pending = session
            .pending
            .lock()
            .map_err(|_| LocalPtyError::Poisoned)?;
        for event in pending.drain(..) {
            let _ = sender.try_send(event);
        }
        Ok(receiver)
    }

    pub fn write(&self, id: u32, bytes: &[u8]) -> Result<(), LocalPtyError> {
        if bytes.len() > 256 * 1024 {
            return Err(LocalPtyError::Io("input payload exceeds 256 KiB".into()));
        }
        let session = self.get(id)?;
        let mut writer = session.writer.lock().map_err(|_| LocalPtyError::Poisoned)?;
        writer
            .write_all(bytes)
            .map_err(|e| LocalPtyError::Io(e.to_string()))?;
        writer.flush().map_err(|e| LocalPtyError::Io(e.to_string()))
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), LocalPtyError> {
        let session = self.get(id)?;
        let result = session
            .master
            .lock()
            .map_err(|_| LocalPtyError::Poisoned)?
            .resize(PtySize {
                rows: rows.clamp(2, 512),
                cols: cols.clamp(2, 512),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| LocalPtyError::Pty(error.to_string()));
        result
    }

    pub fn close(&self, id: u32) -> Result<(), LocalPtyError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| LocalPtyError::Poisoned)?
            .remove(&id)
            .ok_or(LocalPtyError::Missing(id))?;
        let mut child = session.child.lock().map_err(|_| LocalPtyError::Poisoned)?;
        let _ = child.kill();
        Ok(())
    }

    pub fn shutdown(&self) {
        let ids = self
            .sessions
            .lock()
            .ok()
            .map(|sessions| sessions.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ids {
            let _ = self.close(id);
        }
    }

    fn get(&self, id: u32) -> Result<Arc<LocalSession>, LocalPtyError> {
        self.sessions
            .lock()
            .map_err(|_| LocalPtyError::Poisoned)?
            .get(&id)
            .cloned()
            .ok_or(LocalPtyError::Missing(id))
    }
}

fn spawn_reader(session: Arc<LocalSession>, mut reader: Box<dyn Read + Send>) {
    let _ = std::thread::Builder::new()
        .name(format!("kodework-local-pty-{}", session.descriptor.id))
        .spawn(move || {
            let mut buffer = vec![0_u8; CHUNK_SIZE];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        emit(
                            &session,
                            LocalTerminalEvent::Exited {
                                code: child_exit_code(&session),
                            },
                        );
                        break;
                    }
                    Ok(size) => emit(
                        &session,
                        LocalTerminalEvent::Data {
                            bytes: buffer[..size].to_vec(),
                        },
                    ),
                    Err(error) => {
                        emit(
                            &session,
                            LocalTerminalEvent::Error {
                                message: error.to_string(),
                            },
                        );
                        break;
                    }
                }
            }
        });
}

fn child_exit_code(session: &LocalSession) -> u32 {
    session
        .child
        .lock()
        .ok()
        .and_then(|mut child| child.try_wait().ok().flatten())
        .map(|status| status.exit_code())
        .unwrap_or(0)
}

fn emit(session: &LocalSession, event: LocalTerminalEvent) {
    let sender = session
        .subscriber
        .lock()
        .ok()
        .and_then(|subscriber| subscriber.clone());
    if let Some(sender) = sender {
        if sender.blocking_send(event.clone()).is_ok() {
            return;
        }
        if let Ok(mut subscriber) = session.subscriber.lock() {
            *subscriber = None;
        }
    }
    if let Ok(mut pending) = session.pending.lock() {
        if pending.len() >= MAX_PENDING_EVENTS {
            pending.pop_front();
        }
        pending.push_back(event);
    }
}

fn command_plan(
    kind: &LocalTerminalKind,
    distribution: Option<&str>,
) -> Result<(OsString, Vec<OsString>, String), LocalPtyError> {
    match kind {
        LocalTerminalKind::PowerShell => {
            // Windows PowerShell is part of the OS and is the most reliable
            // ConPTY target. Developer-tool runtimes can inject a private
            // pwsh.exe into PATH that is discoverable but not independently
            // launchable outside that tool's process environment.
            let program = resolve_command("powershell.exe")
                .or_else(|| resolve_command("pwsh.exe"))
                .ok_or_else(|| LocalPtyError::Pty("PowerShell 未安装或不在 PATH 中".into()))?;
            Ok((program, vec!["-NoLogo".into()], "PowerShell".into()))
        }
        LocalTerminalKind::CommandPrompt => Ok((
            resolve_command("cmd.exe")
                .ok_or_else(|| LocalPtyError::Pty("cmd.exe 不在 PATH 中".into()))?,
            vec!["/Q".into()],
            "命令提示符".into(),
        )),
        LocalTerminalKind::Wsl => {
            if !command_exists("wsl.exe") {
                return Err(LocalPtyError::Pty(
                    "WSL 未安装或 wsl.exe 不在 PATH 中".into(),
                ));
            }
            let name = distribution
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| LocalPtyError::InvalidDistribution("未选择 WSL 发行版".into()))?;
            let names = list_wsl_distributions();
            if !names.iter().any(|item| item == name) {
                return Err(LocalPtyError::InvalidDistribution(name.into()));
            }
            Ok((
                resolve_command("wsl.exe")
                    .ok_or_else(|| LocalPtyError::Pty("wsl.exe 不在 PATH 中".into()))?,
                vec!["--distribution".into(), name.into()],
                format!("WSL · {name}"),
            ))
        }
    }
}

fn command_exists(program: &str) -> bool {
    resolve_command(program).is_some()
}

fn resolve_command(program: &str) -> Option<OsString> {
    #[cfg(windows)]
    {
        let output = hidden_command("where.exe").arg(program).output().ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|candidate| !candidate.is_empty() && Path::new(candidate).is_file())
            .map(OsString::from)
    }
    #[cfg(not(windows))]
    {
        let output = hidden_command("which").arg(program).output().ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|candidate| !candidate.is_empty() && Path::new(candidate).is_file())
            .map(OsString::from)
    }
}

#[cfg(windows)]
fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(0x0800_0000);
    command
}
#[cfg(not(windows))]
fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

fn list_wsl_distributions() -> Vec<String> {
    let Some(executable) = resolve_command("wsl.exe") else {
        return Vec::new();
    };
    let output = match hidden_command(&executable)
        .args(["--list", "--quiet"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }
    decode_wsl_distributions(&output.stdout)
}

fn decode_wsl_distributions(input: &[u8]) -> Vec<String> {
    let mut bytes = input.to_vec();
    if bytes.starts_with(&[0xFF, 0xFE]) {
        bytes = bytes[2..].to_vec();
    }
    let text = if bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).all(|byte| *byte == 0) {
        String::from_utf16_lossy(
            &bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    text.lines()
        .map(|line| {
            line.trim_matches('\0')
                .trim()
                .trim_start_matches('*')
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::decode_wsl_distributions;

    #[cfg(windows)]
    async fn assert_real_terminal_round_trip(
        kind: super::LocalTerminalKind,
        input: &[u8],
        sentinel: &str,
    ) -> Result<(), String> {
        let manager = super::LocalTerminalManager::new();
        let descriptor = manager
            .open(kind, None, 80, 24)
            .map_err(|error| error.to_string())?;
        let mut events = manager
            .subscribe(descriptor.id)
            .map_err(|error| error.to_string())?;
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        manager
            .write(descriptor.id, input)
            .map_err(|error| error.to_string())?;
        let output = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut output = Vec::new();
            while let Some(event) = events.recv().await {
                match event {
                    super::LocalTerminalEvent::Data { bytes } => {
                        output.extend(bytes);
                        if String::from_utf8_lossy(&output).contains(sentinel) {
                            return Ok(output);
                        }
                    }
                    super::LocalTerminalEvent::Exited { code } => {
                        return Err(format!(
                            "local terminal exited with {code}; output was {}",
                            String::from_utf8_lossy(&output),
                        ))
                    }
                    super::LocalTerminalEvent::Error { message } => {
                        return Err(format!("local PTY read failed: {message}"));
                    }
                }
            }
            Ok(output)
        })
        .await
        .map_err(|_| "local terminal should produce output before timeout".to_string())??;
        assert!(String::from_utf8_lossy(&output).contains(sentinel));
        manager.shutdown();
        Ok(())
    }

    #[test]
    fn wsl_utf16_parser_handles_bom_and_markers() {
        let bytes = "Ubuntu\r\nDebian\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut bom = vec![0xff, 0xfe];
        bom.extend(bytes);
        assert_eq!(decode_wsl_distributions(&bom), vec!["Ubuntu", "Debian"]);
        assert_eq!(
            decode_wsl_distributions(b"* Ubuntu\r\n\r\nDebian\n"),
            vec!["Ubuntu", "Debian"]
        );
    }

    #[test]
    fn wsl_parser_rejects_empty_and_ignores_nul_lines() {
        assert!(decode_wsl_distributions(b"\0\r\n\n").is_empty());
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "current_thread")]
    async fn powershell_round_trip_uses_real_conpty() -> Result<(), String> {
        if !super::command_exists("pwsh.exe") && !super::command_exists("powershell.exe") {
            return Ok(());
        }
        assert_real_terminal_round_trip(
            super::LocalTerminalKind::PowerShell,
            // PowerShell asks for the cursor position during startup.
            // xterm.js answers this automatically; this headless test
            // emulates the same device-status response.
            b"\x1b[1;1RWrite-Output KODEWORK_LOCAL_PTY_SENTINEL\rexit\r",
            "KODEWORK_LOCAL_PTY_SENTINEL",
        )
        .await
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "current_thread")]
    async fn command_prompt_round_trip_uses_real_conpty() -> Result<(), String> {
        if !super::command_exists("cmd.exe") {
            return Ok(());
        }
        assert_real_terminal_round_trip(
            super::LocalTerminalKind::CommandPrompt,
            b"\x1b[1;1Recho KODEWORK_CMD_PTY_SENTINEL\rexit\r",
            "KODEWORK_CMD_PTY_SENTINEL",
        )
        .await
    }

    #[cfg(windows)]
    #[test]
    fn closed_terminal_rejects_io_and_resize() -> Result<(), super::LocalPtyError> {
        if !super::command_exists("cmd.exe") {
            return Ok(());
        }
        let manager = super::LocalTerminalManager::new();
        let terminal = manager.open(super::LocalTerminalKind::CommandPrompt, None, 80, 24)?;
        manager.close(terminal.id)?;
        assert!(matches!(
            manager.write(terminal.id, b"echo should-not-run\r"),
            Err(super::LocalPtyError::Missing(id)) if id == terminal.id
        ));
        assert!(matches!(
            manager.resize(terminal.id, 80, 24),
            Err(super::LocalPtyError::Missing(id)) if id == terminal.id
        ));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn rapid_resize_is_bounded_and_session_limit_is_enforced() -> Result<(), super::LocalPtyError> {
        if !super::command_exists("cmd.exe") {
            return Ok(());
        }
        let manager = super::LocalTerminalManager::new();
        let mut ids = Vec::new();
        for index in 0..super::MAX_SESSIONS {
            let terminal = manager.open(
                super::LocalTerminalKind::CommandPrompt,
                None,
                if index == 0 { 0 } else { 80 },
                if index == 0 { u16::MAX } else { 24 },
            )?;
            ids.push(terminal.id);
        }
        let primary = match ids.first() {
            Some(id) => *id,
            None => unreachable!("at least one terminal was opened"),
        };
        for size in 0..100_u16 {
            manager.resize(primary, size, u16::MAX - size)?;
        }
        assert!(matches!(
            manager.open(super::LocalTerminalKind::CommandPrompt, None, 80, 24),
            Err(super::LocalPtyError::LimitReached)
        ));
        manager.shutdown();
        Ok(())
    }
}
