//! End-to-end agent protocol test: spawn the real binary, speak the
//! token-gated JSON protocol over TCP, and verify responses.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static TOKEN_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn agent_binary() -> &'static str {
    env!("CARGO_BIN_EXE_kodework-agent")
}

fn token_file(token: &str) -> PathBuf {
    let serial = TOKEN_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "kodework-agent-token-{}-{serial}",
        std::process::id()
    ));
    std::fs::write(&path, format!("{token}\n"))
        .unwrap_or_else(|error| unreachable!("write token file: {error}"));
    path
}

fn spawn_agent(token: &str) -> (std::process::Child, std::net::SocketAddr, PathBuf) {
    let token_path = token_file(token);
    let mut child = Command::new(agent_binary())
        .args(["--port", "0", "--token-file"])
        .arg(&token_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| unreachable!("spawn agent: {error}"));
    // Read the bound address from stdout.
    let stdout = child
        .stdout
        .take()
        .unwrap_or_else(|| unreachable!("stdout"));
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .unwrap_or_else(|error| unreachable!("read addr: {error}"));
    let addr: std::net::SocketAddr = line
        .trim()
        .parse()
        .unwrap_or_else(|error| unreachable!("parse addr {line:?}: {error}"));
    // Keep the reader alive on a thread so the agent can finish.
    std::thread::spawn(move || {
        let _ = reader;
        std::thread::sleep(Duration::from_secs(30));
    });
    (child, addr, token_path)
}

fn request(addr: &std::net::SocketAddr, token: &str, payload: &str) -> String {
    let stream = TcpStream::connect(addr).unwrap_or_else(|error| unreachable!("connect: {error}"));
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap_or_else(|error| unreachable!("timeout: {error}"));
    let mut stream = stream;
    stream
        .write_all(format!("{token}\n{payload}\n").as_bytes())
        .unwrap_or_else(|error| unreachable!("write: {error}"));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .unwrap_or_else(|error| unreachable!("read: {error}"));
    line
}

#[test]
fn status_and_exec_round_trip() {
    let token = "test-token-1234567890-abcdefghijk";
    let (mut child, addr, token_path) = spawn_agent(token);

    let status = request(&addr, token, r#"{"cmd":"status"}"#);
    assert!(status.contains("\"ok\":true"), "got: {status}");
    assert!(status.contains("\"version\""), "got: {status}");

    let exec_command = if cfg!(windows) {
        "echo agent-ok"
    } else {
        "printf agent-ok"
    };
    let exec = request(
        &addr,
        token,
        &format!(r#"{{"cmd":"exec","command":"{exec_command}"}}"#),
    );
    assert!(exec.contains("\"ok\":true"), "got: {exec}");
    assert!(exec.contains("agent-ok"), "got: {exec}");
    assert!(exec.contains("\"exit_code\":0"), "got: {exec}");

    // Wrong token is rejected and the connection is closed.
    let rejected = request(&addr, "wrong-token", r#"{"cmd":"status"}"#);
    assert!(rejected.contains("\"ok\":false"), "got: {rejected}");

    let _ = child.kill();
    let _ = std::fs::remove_file(token_path);
}

#[test]
fn exec_timeout_is_bounded() {
    let token = "timeout-token-1234567890-abcdefgh";
    let (mut child, addr, token_path) = spawn_agent(token);
    let slow_command = if cfg!(windows) {
        "ping -n 6 127.0.0.1 >nul"
    } else {
        "sleep 5"
    };
    let exec = request(
        &addr,
        token,
        &format!(r#"{{"cmd":"exec","command":"{slow_command}","timeout_secs":1}}"#),
    );
    assert!(exec.contains("timed out"), "got: {exec}");
    let _ = child.kill();
    let _ = std::fs::remove_file(token_path);
}

#[test]
fn missing_token_is_rejected_before_binding() {
    let output = Command::new(agent_binary())
        .args(["--port", "0"])
        .output()
        .unwrap_or_else(|error| unreachable!("spawn agent: {error}"));
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("set --token-file"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
