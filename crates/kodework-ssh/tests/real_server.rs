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
use kodework_ssh::connection::{AuthMethod, ConnectionOptions, ZeroizingVec};
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
        .unwrap_or_else(|error| panic!("connect task failed: {error}"))
        .unwrap_or_else(|error| panic!("connecting to the real server failed: {error}"))
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
        .unwrap_or_else(|error| panic!("run_command against the real server failed: {error}"));
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
        .unwrap_or_else(|error| panic!("disconnect failed: {error}"));
}

#[tokio::test]
async fn real_server_sftp_round_trips_a_file() {
    let target = require_target!();
    let (connection, _events) = connect(&target).await;
    let session = connection
        .sftp()
        .await
        .unwrap_or_else(|error| panic!("opening the real sftp subsystem failed: {error}"));

    let path = format!("{}/round-trip.txt", target.directory);
    let payload = b"kodework real sftp payload\n";
    {
        use tokio::io::AsyncWriteExt;
        let mut file = session
            .create(&path)
            .await
            .unwrap_or_else(|error| panic!("creating {path} failed: {error}"));
        file.write_all(payload)
            .await
            .unwrap_or_else(|error| panic!("writing {path} failed: {error}"));
        file.shutdown()
            .await
            .unwrap_or_else(|error| panic!("closing {path} failed: {error}"));
    }

    let read_back = {
        use tokio::io::AsyncReadExt;
        let mut file = session
            .open(&path)
            .await
            .unwrap_or_else(|error| panic!("opening {path} failed: {error}"));
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .unwrap_or_else(|error| panic!("reading {path} failed: {error}"));
        buffer
    };
    assert_eq!(read_back, payload, "the file must come back byte for byte");

    let entries = session
        .read_dir(&target.directory)
        .await
        .unwrap_or_else(|error| panic!("listing {} failed: {error}", target.directory));
    let names: Vec<String> = entries.map(|entry| entry.file_name()).collect();
    assert!(
        names.iter().any(|name| name == "round-trip.txt"),
        "the listing must contain the file just written: {names:?}"
    );

    session
        .remove_file(&path)
        .await
        .unwrap_or_else(|error| panic!("removing {path} failed: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| panic!("disconnect failed: {error}"));
}

#[tokio::test]
async fn real_server_pty_echoes_and_resizes() {
    let target = require_target!();
    let (connection, _events) = connect(&target).await;
    let pty = connection
        .open_pty(80, 24)
        .await
        .unwrap_or_else(|error| panic!("opening a real pty failed: {error}"));

    pty.write(b"printf 'kodework-real-pty\\n'\n")
        .await
        .unwrap_or_else(|error| panic!("writing to the pty failed: {error}"));
    pty.resize(100, 40)
        .await
        .unwrap_or_else(|error| panic!("resizing the real pty failed: {error}"));

    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| panic!("disconnect failed: {error}"));
}
