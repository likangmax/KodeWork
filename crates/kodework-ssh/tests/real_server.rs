//! End-to-end tests against a **real** SSH server.
//!
//! The fake-server tests in `fake_server.rs` use an in-process russh server, which
//! is fast and hermetic but shares this workspace's crate versions and
//! assumptions. These tests talk to an actual OpenSSH `sshd`, so they cover what a
//! fake server cannot: the real banner and key exchange, real `authorized_keys`
//! authentication, a real `sftp` subsystem, and a real shell.
//!
//! They are skipped unless the environment points at a server, so `cargo test`
//! stays usable without one. In CI the `real-ssh` job starts an `sshd` on the
//! runner and sets these variables:
//!
//! - `KODEWORK_TEST_SSH_HOST` (default `127.0.0.1`)
//! - `KODEWORK_TEST_SSH_PORT` (default `2222`)
//! - `KODEWORK_TEST_SSH_USER` (required; the account the key belongs to)
//! - `KODEWORK_TEST_SSH_KEY` (required; path to the private key)
//! - `KODEWORK_TEST_SSH_DIR` (default `/tmp/kodework-real-ssh`; must be writable)
use kodework_ssh::connection::{AuthMethod, ConnectionOptions};
use kodework_ssh::handler::SessionEvent;
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_ssh::SshConnection;
use std::sync::Arc;
use std::time::Duration;

struct Target {
    host: String,
    port: u16,
    user: String,
    key: std::path::PathBuf,
    directory: String,
}

fn target() -> Option<Target> {
    let user = std::env::var("KODEWORK_TEST_SSH_USER").ok()?;
    let key = std::env::var("KODEWORK_TEST_SSH_KEY").ok()?;
    if user.is_empty() || key.is_empty() {
        return None;
    }
    Some(Target {
        host: std::env::var("KODEWORK_TEST_SSH_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("KODEWORK_TEST_SSH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2222),
        user,
        key: key.into(),
        directory: std::env::var("KODEWORK_TEST_SSH_DIR")
            .unwrap_or_else(|_| "/tmp/kodework-real-ssh".into()),
    })
}

/// The macro keeps the "no server configured" path explicit in the output instead
/// of silently passing.
macro_rules! require_target {
    () => {
        match target() {
            Some(target) => target,
            None => {
                eprintln!(
                    "skipped: set KODEWORK_TEST_SSH_USER and KODEWORK_TEST_SSH_KEY to run against a real server"
                );
                return;
            }
        }
    };
}

async fn connect(target: &Target) -> (SshConnection, tokio::sync::mpsc::Receiver<SessionEvent>) {
    let broker = Arc::new(HostKeyBroker::new(
        Arc::new(MemoryKnownHosts::new()),
        Duration::from_secs(10),
    ));
    let options = ConnectionOptions::new(
        target.host.clone(),
        target.port,
        target.user.clone(),
        vec![AuthMethod::PublicKey {
            key_path: target.key.clone(),
            passphrase: None,
        }],
        Arc::clone(&broker),
        1,
    );
    let task = tokio::spawn(async move { SshConnection::connect(options).await });
    for _ in 0..400 {
        let requests = broker.drain_requests();
        if let Some(request) = requests.first() {
            // A real server presents a real key; trusting it once is what a user
            // does after checking the fingerprint.
            assert!(
                broker.answer(request.request_id, HostKeyDecision::TrustOnce),
                "the host-key decision must reach the pending request"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.await
        .unwrap_or_else(|error| unreachable!("connect task failed: {error}"))
        .unwrap_or_else(|error| unreachable!("connecting to the real server failed: {error}"))
}

#[tokio::test]
async fn real_server_runs_a_command_and_reports_its_exit_code() {
    let target = require_target!();
    let (connection, _events) = connect(&target).await;

    let output = connection
        .run_command(
            "printf 'kodework-real-ssh'; exit 7",
            Duration::from_secs(30),
            64 * 1024,
        )
        .await
        .unwrap_or_else(|error| {
            unreachable!("run_command against the real server failed: {error}")
        });
    assert_eq!(
        output.exit_code,
        Some(7),
        "the real exit status must survive"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("kodework-real-ssh"),
        "stdout was: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );

    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect failed: {error}"));
}

#[tokio::test]
async fn real_server_sftp_round_trips_a_file() {
    let target = require_target!();
    let (connection, _events) = connect(&target).await;
    let session = connection
        .sftp()
        .await
        .unwrap_or_else(|error| unreachable!("opening the real sftp subsystem failed: {error}"));

    // These tests can be pointed at a server someone cares about, so every path
    // they touch is created for this run and removed by it: a fixed filename would
    // let `create` replace an existing file and then delete it.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let directory = format!(
        "{}/kodework-test-{}-{}",
        target.directory,
        std::process::id(),
        stamp
    );
    session
        .create_dir(&directory)
        .await
        .unwrap_or_else(|error| unreachable!("creating {directory} failed: {error}"));

    let path = format!("{directory}/round-trip.txt");
    let payload = b"kodework real sftp payload\n";
    {
        use tokio::io::AsyncWriteExt;
        let mut file = session
            .create(&path)
            .await
            .unwrap_or_else(|error| unreachable!("creating {path} failed: {error}"));
        file.write_all(payload)
            .await
            .unwrap_or_else(|error| unreachable!("writing {path} failed: {error}"));
        file.shutdown()
            .await
            .unwrap_or_else(|error| unreachable!("closing {path} failed: {error}"));
    }

    let read_back = {
        use tokio::io::AsyncReadExt;
        let mut file = session
            .open(&path)
            .await
            .unwrap_or_else(|error| unreachable!("opening {path} failed: {error}"));
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .unwrap_or_else(|error| unreachable!("reading {path} failed: {error}"));
        buffer
    };
    assert_eq!(read_back, payload, "the file must come back byte for byte");

    let entries = session
        .read_dir(&directory)
        .await
        .unwrap_or_else(|error| unreachable!("listing {directory} failed: {error}"));
    let names: Vec<String> = entries.map(|entry| entry.file_name()).collect();
    assert_eq!(
        names,
        vec!["round-trip.txt".to_string()],
        "the directory created for this run must hold exactly the file just written"
    );

    session
        .remove_file(&path)
        .await
        .unwrap_or_else(|error| unreachable!("removing {path} failed: {error}"));
    session
        .remove_dir(&directory)
        .await
        .unwrap_or_else(|error| unreachable!("removing {directory} failed: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect failed: {error}"));
}

/// Asserts what the test is named for: the shell echoes the command, reports the
/// size the client resized it to, and that output reaches the event stream.
///
/// The marker is assembled remotely (`printf 'KODEWORK_%s' REAL_PTY_OK`) so a
/// match cannot come from the terminal echoing the typed line back.
#[tokio::test]
async fn real_server_pty_echoes_and_resizes() {
    let target = require_target!();
    let (connection, mut events) = connect(&target).await;
    let pty = connection
        .open_pty(80, 24)
        .await
        .unwrap_or_else(|error| unreachable!("opening a real pty failed: {error}"));
    let channel = pty.channel_id();

    // Resize before the command so the reported size is the resized one.
    pty.resize(100, 40)
        .await
        .unwrap_or_else(|error| unreachable!("resizing the real pty failed: {error}"));
    pty.write(b"printf 'KODEWORK_%s\\n' REAL_PTY_OK; stty size\n")
        .await
        .unwrap_or_else(|error| unreachable!("writing to the pty failed: {error}"));

    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, events.recv()).await {
            Ok(Some(SessionEvent::Data {
                channel: event_channel,
                bytes,
            })) if event_channel == channel => {
                seen.extend_from_slice(&bytes);
                let text = String::from_utf8_lossy(&seen);
                if text.contains("KODEWORK_REAL_PTY_OK") && text.contains("40 100") {
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    let output = String::from_utf8_lossy(&seen).into_owned();
    assert!(
        output.contains("KODEWORK_REAL_PTY_OK"),
        "the shell's output must reach the event stream, saw: {output:?}"
    );
    assert!(
        output.contains("40 100"),
        "the shell must report the resized dimensions (rows cols), saw: {output:?}"
    );

    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect failed: {error}"));
}
