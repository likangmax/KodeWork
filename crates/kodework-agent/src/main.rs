#![forbid(unsafe_code)]

//! Optional remote agent: a tiny JSON-over-TCP service deployed to a
//! remote host. Loopback-only, token-gated, bounded output. Used by
//! Kodework when present (never required).

use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

const MAX_OUTPUT: usize = 256 * 1024;
const MAX_REQUEST_LINE: usize = 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 4096;
const MIN_TOKEN_BYTES: usize = 32;
const MAX_CONNECTIONS: usize = 32;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 60 * 60;
const PROTOCOL_READ_TIMEOUT: Duration = Duration::from_secs(10);

struct Config {
    port: u16,
    token: Zeroizing<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    Status,
    Exec {
        command: String,
        timeout_secs: Option<u64>,
    },
    TmuxSessions,
}

#[derive(Debug, Clone, Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions: Option<Vec<String>>,
}

impl Response {
    fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            hostname: None,
            version: None,
            exit_code: None,
            stdout: None,
            stderr: None,
            truncated: None,
            sessions: None,
        }
    }
    fn err(message: impl Into<String>) -> Self {
        let mut response = Self::ok();
        response.ok = false;
        response.error = Some(message.into());
        response
    }
}

#[tokio::main]
async fn main() {
    let config = parse_config().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    let token = Arc::new(config.token);
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let bind: std::net::SocketAddr =
        format!("127.0.0.1:{}", config.port)
            .parse()
            .unwrap_or_else(|error| {
                eprintln!("invalid port: {error}");
                std::process::exit(2);
            });
    let listener = TcpListener::bind(bind).await.unwrap_or_else(|error| {
        eprintln!("bind failed: {error}");
        std::process::exit(2);
    });
    let actual = listener.local_addr().unwrap_or_else(|error| {
        eprintln!("local_addr failed: {error}");
        std::process::exit(2);
    });
    // Print the bound address so a parent process can discover it.
    // Flush: stdout is fully buffered when piped.
    println!("{}", actual);
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let token = Arc::clone(&token);
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = handle_connection(stream, token.as_str()).await;
                });
            }
            Err(error) => eprintln!("accept failed: {error}"),
        }
    }
}

fn parse_config() -> Result<Config, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut port: u16 = 0;
    let mut token_file: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                let value = args.get(i).ok_or("--port requires a value")?;
                port = value
                    .parse()
                    .map_err(|error| format!("invalid --port value: {error}"))?;
            }
            "--token-file" => {
                i += 1;
                let value = args.get(i).ok_or("--token-file requires a value")?;
                token_file = Some(PathBuf::from(value));
            }
            value => return Err(format!("unknown argument: {value}")),
        }
        i += 1;
    }
    let mut token = Zeroizing::new(match token_file {
        Some(path) => std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read token file {}: {error}", path.display()))?,
        None => env::var("KODEWORK_AGENT_TOKEN")
            .map_err(|_| "set --token-file or KODEWORK_AGENT_TOKEN".to_string())?,
    });
    while matches!(token.as_bytes().last(), Some(b'\n' | b'\r')) {
        token.pop();
    }
    validate_token(&token)?;
    Ok(Config { port, token })
}

fn validate_token(token: &str) -> Result<(), String> {
    if !(MIN_TOKEN_BYTES..=MAX_TOKEN_BYTES).contains(&token.len()) {
        return Err(format!(
            "token length must be between {MIN_TOKEN_BYTES} and {MAX_TOKEN_BYTES} bytes"
        ));
    }
    if token.chars().any(char::is_whitespace) || token.chars().any(char::is_control) {
        return Err("token must not contain whitespace or control characters".into());
    }
    Ok(())
}

async fn handle_connection(stream: TcpStream, token: &str) -> Result<(), std::io::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let provided_token = read_line_limited(&mut reader, MAX_TOKEN_BYTES).await?;
    if !token_matches(&provided_token, token) {
        return write_response(&mut writer, &Response::err("invalid token")).await;
    }
    let request_line = read_line_limited(&mut reader, MAX_REQUEST_LINE).await?;
    let response = dispatch(&request_line).await;
    write_response(&mut writer, &response).await
}

async fn read_line_limited<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<String, std::io::Error> {
    let mut bytes = Vec::new();
    let read = tokio::time::timeout(PROTOCOL_READ_TIMEOUT, async {
        reader
            .take(max_bytes as u64 + 1)
            .read_until(b'\n', &mut bytes)
            .await
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "protocol read timed out"))??;
    if read == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "connection closed before a complete line",
        ));
    }
    if bytes.len() > max_bytes || !bytes.ends_with(b"\n") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "protocol line exceeds the size limit",
        ));
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "line is not UTF-8"))
}

fn token_matches(provided: &str, expected: &str) -> bool {
    use subtle::ConstantTimeEq;
    provided.len() == expected.len() && bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
}

async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: &Response,
) -> Result<(), std::io::Error> {
    let mut payload = serde_json::to_string(response)
        .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialize\"}".to_string());
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await
}

async fn dispatch(request_line: &str) -> Response {
    let request: Request = match serde_json::from_str(request_line) {
        Ok(request) => request,
        Err(error) => return Response::err(format!("bad request: {error}")),
    };
    match request {
        Request::Status => {
            let mut response = Response::ok();
            response.hostname = hostname();
            response.version = Some(env!("CARGO_PKG_VERSION").to_string());
            response
        }
        Request::Exec {
            command,
            timeout_secs,
        } => run_exec(&command, timeout_secs).await,
        Request::TmuxSessions => {
            let output = tokio::process::Command::new("tmux")
                .args(["ls", "-F", "#{session_name}"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .await;
            let mut response = Response::ok();
            match output {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    response.sessions = Some(text.lines().map(str::to_string).collect::<Vec<_>>());
                }
                _ => {
                    response.ok = false;
                    response.error = Some("tmux unavailable".to_string());
                }
            }
            response
        }
    }
}

async fn run_exec(command: &str, timeout_secs: Option<u64>) -> Response {
    if command.is_empty() || command.len() > MAX_REQUEST_LINE || command.contains('\0') {
        return Response::err("command is empty or exceeds the size limit");
    }
    let timeout = Duration::from_secs(
        timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS),
    );
    // Cross-platform: the agent is deployed on Linux remotes, but the
    // integration tests run on Windows hosts too.
    #[cfg(windows)]
    let mut command_builder = {
        let mut builder = tokio::process::Command::new("cmd");
        builder.arg("/C").arg(command);
        builder
    };
    #[cfg(not(windows))]
    let mut command_builder = {
        let mut builder = tokio::process::Command::new("sh");
        builder.arg("-c").arg(command);
        builder
    };
    let mut child = match command_builder
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Response::err(format!("spawn failed: {error}")),
    };
    let mut stdout_reader = child.stdout.take();
    let mut stderr_reader = child.stderr.take();
    let outcome = tokio::time::timeout(timeout, async {
        let stdout_future = async {
            match stdout_reader.as_mut() {
                Some(reader) => capture(reader).await,
                None => (String::new(), false),
            }
        };
        let stderr_future = async {
            match stderr_reader.as_mut() {
                Some(reader) => capture(reader).await,
                None => (String::new(), false),
            }
        };
        let (stdout_text, stderr_text) = tokio::join!(stdout_future, stderr_future);
        let status = child.wait().await;
        (status, stdout_text, stderr_text)
    })
    .await;
    let mut response = Response::ok();
    match outcome {
        Err(_) => {
            let _ = child.kill().await;
            response.ok = false;
            response.error = Some(format!("timed out after {timeout:?}"));
        }
        Ok((status, stdout_text, stderr_text)) => {
            response.exit_code = status.ok().and_then(|s| s.code());
            response.stdout = Some(stdout_text.0);
            response.stderr = Some(stderr_text.0);
            response.truncated = Some(stdout_text.1 || stderr_text.1);
        }
    }
    response
}

async fn capture<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> (String, bool) {
    use tokio::io::AsyncReadExt;
    let mut buffer = Vec::new();
    let mut temp = [0u8; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut temp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buffer.len() + n > MAX_OUTPUT {
                    truncated = true;
                } else {
                    buffer.extend_from_slice(&temp[..n]);
                }
            }
        }
    }
    (String::from_utf8_lossy(&buffer).into_owned(), truncated)
}

fn hostname() -> Option<String> {
    let unix = std::fs::read_to_string("/etc/hostname")
        .map(|value| value.trim().to_string())
        .ok()
        .or_else(|| env::var("HOSTNAME").ok());
    #[cfg(windows)]
    let unix = unix.or_else(|| env::var("COMPUTERNAME").ok());
    unix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests() {
        let status: Request = serde_json::from_str(r#"{"cmd":"status"}"#)
            .unwrap_or_else(|error| unreachable!("status parse: {error}"));
        assert!(matches!(status, Request::Status));
        let exec: Request =
            serde_json::from_str(r#"{"cmd":"exec","command":"ls","timeout_secs":5}"#)
                .unwrap_or_else(|error| unreachable!("exec parse: {error}"));
        match exec {
            Request::Exec {
                command,
                timeout_secs,
            } => {
                assert_eq!(command, "ls");
                assert_eq!(timeout_secs, Some(5));
            }
            _ => unreachable!("exec"),
        }
        let bad: Result<Request, _> = serde_json::from_str(r#"{"cmd":"nope"}"#);
        assert!(bad.is_err());
    }

    #[test]
    fn responses_serialize_ok() {
        let response = Response::ok();
        let json = serde_json::to_string(&response)
            .unwrap_or_else(|error| unreachable!("serialize: {error}"));
        assert!(json.contains("\"ok\":true"));
    }
}
