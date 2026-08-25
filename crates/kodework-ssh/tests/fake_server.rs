//! Integration tests against the kodework-testkit fake SSH server.
//! Covers: auth success/failure, host-key trust flows (once/save/changed),
//! exec output, byte-flood integrity, transport drop and refused connects.

use kodework_ssh::connection::{AuthMethod, ConnectionOptions, ZeroizingVec};
use kodework_ssh::handler::SessionEvent;
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_ssh::SshConnection;
use kodework_testkit::fake_ssh::{
    FakeExecBehavior, FakeExecResponse, FakeShellBehavior, FakeSshOptions, FakeSshServer,
};
use russh::keys::ssh_key::{Algorithm, LineEnding, PrivateKey};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const USER: &str = "tester";
const PASSWORD: &str = "test-password";

fn options_for(
    port: u16,
    password: &str,
    broker: Arc<HostKeyBroker>,
    generation: u64,
) -> ConnectionOptions {
    ConnectionOptions::new(
        "127.0.0.1".into(),
        port,
        USER.into(),
        vec![AuthMethod::Password(ZeroizingVec::new(
            password.as_bytes().to_vec(),
        ))],
        broker,
        generation,
    )
}

fn new_broker() -> Arc<HostKeyBroker> {
    Arc::new(HostKeyBroker::new(
        Arc::new(MemoryKnownHosts::new()),
        Duration::from_secs(10),
    ))
}

/// Connects while answering the pending host-key request on the broker.
async fn connect_with_trust(
    options: ConnectionOptions,
    broker: Arc<HostKeyBroker>,
    decision: HostKeyDecision,
) -> Result<(SshConnection, mpsc::Receiver<SessionEvent>), kodework_ssh::SshError> {
    let task = tokio::spawn(async move { SshConnection::connect(options).await });
    let mut requests = Vec::new();
    for _ in 0..200 {
        requests = broker.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(requests.len(), 1, "exactly one host-key request expected");
    assert!(
        broker.answer(requests[0].request_id, decision),
        "answer must reach the pending request"
    );
    task.await
        .unwrap_or_else(|error| unreachable!("connect task join failed: {error}"))
}

/// Collects events for up to `timeout`, stopping early on channel close.
async fn collect_events(
    rx: &mut mpsc::Receiver<SessionEvent>,
    timeout: Duration,
) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(event)) => {
                let done = matches!(
                    event,
                    SessionEvent::Disconnected { .. }
                        | SessionEvent::ChannelClosed { .. }
                        | SessionEvent::Error { .. }
                );
                events.push(event);
                if done {
                    break;
                }
            }
            _ => break,
        }
    }
    events
}

#[tokio::test]
async fn custom_ed25519_private_key_authenticates() {
    let server = FakeSshServer::start(FakeSshOptions {
        password: None,
        accept_publickey: true,
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let key_path = std::env::temp_dir().join(format!(
        "kodework-test-key-{}-{}.pem",
        std::process::id(),
        server.addr().port()
    ));
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .unwrap_or_else(|error| unreachable!("generate key: {error}"));
    key.write_openssh_file(&key_path, LineEnding::LF)
        .unwrap_or_else(|error| unreachable!("write key: {error}"));
    let broker = new_broker();
    let options = ConnectionOptions::new(
        "127.0.0.1".into(),
        server.addr().port(),
        USER.into(),
        vec![AuthMethod::PublicKey {
            key_path: key_path.clone(),
            passphrase: None,
        }],
        Arc::clone(&broker),
        1,
    );
    let result = connect_with_trust(options, Arc::clone(&broker), HostKeyDecision::TrustOnce).await;
    let _ = std::fs::remove_file(&key_path);
    let (connection, _events) = result.unwrap_or_else(|error| unreachable!("connect: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

fn data_bytes(events: &[SessionEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    for event in events {
        if let SessionEvent::Data { bytes, .. } = event {
            out.extend_from_slice(bytes);
        }
    }
    out
}

#[tokio::test]
async fn password_auth_pty_echo_roundtrip() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, mut rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect failed: {error}"));

    let pty = connection
        .open_pty(120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pty failed: {error}"));
    pty.write(b"hello")
        .await
        .unwrap_or_else(|error| unreachable!("write failed: {error}"));

    let events = collect_events(&mut rx, Duration::from_secs(3)).await;
    let output = data_bytes(&events);
    assert!(
        output.windows(7).any(|window| window == b"hello\r\n"),
        "echoed output must contain the input: {output:?}"
    );
    pty.close()
        .await
        .unwrap_or_else(|error| unreachable!("pty close failed: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect failed: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn host_key_trust_once_requires_decision_every_time() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();

    let (connection, mut _rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("first connect: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));

    // TrustOnce does not persist: the second connect must prompt again.
    let (connection, mut _rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 2),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("second connect: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn host_key_trust_and_save_skips_prompt_on_reconnect() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();

    let (connection, mut _rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustAndSave,
    )
    .await
    .unwrap_or_else(|error| unreachable!("first connect: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));

    // Saved key: reconnect must succeed without any prompt.
    let port = server.addr().port();
    let task = tokio::spawn({
        let broker = Arc::clone(&broker);
        async move { SshConnection::connect(options_for(port, PASSWORD, broker, 2)).await }
    });
    let (connection, mut _rx) = task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("reconnect: {error}"));
    assert!(
        broker.drain_requests().is_empty(),
        "saved host key must not prompt"
    );
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn host_key_change_is_hard_failure() {
    let broker = new_broker();
    let mut rng = rand::rng();
    let key_a = russh::keys::PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .unwrap_or_else(|error| unreachable!("key A: {error}"));
    let key_b = russh::keys::PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .unwrap_or_else(|error| unreachable!("key B: {error}"));

    let server_a = FakeSshServer::start_with_key_on_port(FakeSshOptions::default(), key_a, 0)
        .await
        .unwrap_or_else(|error| unreachable!("server A: {error}"));
    let port = server_a.addr().port();

    let (connection, mut _rx) = connect_with_trust(
        options_for(port, PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustAndSave,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect to A: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server_a.shutdown().await;

    // Same port, different key: the saved key no longer matches.
    let server_b = FakeSshServer::start_with_key_on_port(FakeSshOptions::default(), key_b, port)
        .await
        .unwrap_or_else(|error| unreachable!("server B: {error}"));

    let result = SshConnection::connect(options_for(port, PASSWORD, Arc::clone(&broker), 2)).await;
    assert!(
        matches!(result, Err(kodework_ssh::SshError::HostKeyChanged)),
        "changed key must hard-fail without a prompt"
    );
    assert!(
        broker.drain_requests().is_empty(),
        "no prompt for changed key"
    );
    server_b.shutdown().await;
}

#[tokio::test]
async fn wrong_password_is_authentication_failure() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let port = server.addr().port();
    let task = tokio::spawn({
        let broker = Arc::clone(&broker);
        async move { SshConnection::connect(options_for(port, "wrong-password", broker, 1)).await }
    });
    // The host key is unknown, so a decision is requested before auth;
    // trust it, then the password must fail.
    let mut requests = Vec::new();
    for _ in 0..200 {
        requests = broker.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !requests.is_empty() {
        broker.answer(requests[0].request_id, HostKeyDecision::TrustOnce);
    }
    let result = task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"));
    assert!(
        matches!(result, Err(kodework_ssh::SshError::AuthenticationFailed)),
        "wrong password must fail authentication, got: {result:?}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn exec_returns_output_and_exit_code() {
    let options = FakeSshOptions {
        exec: FakeExecBehavior::Fixed {
            output: b"hello world\n".to_vec(),
            exit_code: 42,
        },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, mut rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let exec = connection
        .exec("echo hello", false, 0, 0)
        .await
        .unwrap_or_else(|error| unreachable!("exec: {error}"));
    let events = collect_events(&mut rx, Duration::from_secs(3)).await;
    let output = data_bytes(&events);
    assert_eq!(output, b"hello world\n");
    let exit = events.iter().find_map(|event| match event {
        SessionEvent::ExitStatus { status, .. } => Some(*status),
        _ => None,
    });
    assert_eq!(exit, Some(42), "exit status must be delivered");
    exec.close()
        .await
        .unwrap_or_else(|error| unreachable!("exec close: {error}"));
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn byte_flood_is_not_lost_and_is_aggregated() {
    // Goal 3.1 terminal throughput baseline: 10 MB without a lost byte.
    const FLOOD_BYTES: usize = 10 * 1024 * 1024;
    let options = FakeSshOptions {
        shell: FakeShellBehavior::Flood { bytes: FLOOD_BYTES },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, mut rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect: {error}"));
    let _pty = connection
        .open_pty(120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pty: {error}"));

    let mut total = 0usize;
    let mut batches = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while total < FLOOD_BYTES && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Some(SessionEvent::Data { bytes, .. })) => {
                total += bytes.len();
                batches += 1;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert_eq!(
        total, FLOOD_BYTES,
        "no byte may be lost (received {total}, batches {batches})"
    );
    assert!(
        batches >= FLOOD_BYTES / (32 * 1024),
        "bounded aggregation expected"
    );
    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn transport_drop_reports_disconnected() {
    let options = FakeSshOptions {
        shell: FakeShellBehavior::DropAfter {
            delay: Duration::from_millis(200),
        },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, mut rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect: {error}"));
    let _pty = connection
        .open_pty(120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pty: {error}"));

    let events = collect_events(&mut rx, Duration::from_secs(3)).await;
    assert!(
        events.iter().any(|event| matches!(
            event,
            SessionEvent::Disconnected { .. } | SessionEvent::Error { .. }
        )),
        "transport drop must surface a disconnect event, got: {events:?}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn connection_refused_is_typed() {
    let broker = new_broker();
    // Find a port with no listener.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap_or_else(|error| unreachable!("probe bind: {error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| unreachable!("probe addr: {error}"))
        .port();
    drop(listener);

    let result = SshConnection::connect(options_for(port, PASSWORD, broker, 1)).await;
    assert!(
        matches!(result, Err(kodework_ssh::SshError::ConnectionRefused)),
        "refused connect must be typed, got: {result:?}"
    );
}
#[tokio::test]
async fn auth_stall_is_bounded_by_connect_timeout() {
    let options = FakeSshOptions {
        auth_delay: Duration::from_secs(30),
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let port = server.addr().port();
    let mut conn_options = options_for(port, PASSWORD, Arc::clone(&broker), 1);
    conn_options.connect_timeout = Duration::from_secs(1);
    let task = tokio::spawn(async move { SshConnection::connect(conn_options).await });

    let mut requests = Vec::new();
    for _ in 0..200 {
        requests = broker.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !requests.is_empty() {
        broker.answer(requests[0].request_id, HostKeyDecision::TrustOnce);
    }

    let started = tokio::time::Instant::now();
    let result = task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"));
    let elapsed = started.elapsed();
    assert!(
        matches!(result, Err(kodework_ssh::SshError::Timeout)),
        "stalled auth must time out, got: {result:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "auth timeout must be bounded, took {elapsed:?}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn run_command_output_is_not_broadcast_to_terminal_events() {
    // run_command must consume its own exec channel; the output must not
    // leak into the terminal event stream (tmux list, herdr queries, and
    // quick/background actions would otherwise garble the PTY view).
    let options = FakeSshOptions {
        exec: FakeExecBehavior::Fixed {
            output: b"secret-output\n".to_vec(),
            exit_code: 0,
        },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, mut rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let output = connection
        .run_command("echo hi", Duration::from_secs(5), 1 << 20)
        .await
        .unwrap_or_else(|error| unreachable!("run_command: {error}"));
    assert_eq!(output.stdout, b"secret-output\n");
    assert_eq!(output.exit_code, Some(0));

    // Give a stray broadcast a moment to arrive, then assert it never did.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let events = collect_events(&mut rx, Duration::from_millis(300)).await;
    let bytes = data_bytes(&events);
    let needle = b"secret-output";
    assert!(
        !bytes.windows(needle.len()).any(|window| window == needle),
        "exec output must not reach the terminal event stream"
    );

    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn run_command_accepts_exit_status_after_eof() {
    let options = FakeSshOptions {
        exec: FakeExecBehavior::EofBeforeExit {
            output: b"herdr 0.8.0\n".to_vec(),
            exit_code: 0,
        },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let (connection, _rx) = connect_with_trust(
        options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1),
        Arc::clone(&broker),
        HostKeyDecision::TrustOnce,
    )
    .await
    .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let output = connection
        .run_command("herdr --version", Duration::from_secs(5), 1 << 20)
        .await
        .unwrap_or_else(|error| unreachable!("run_command: {error}"));
    assert_eq!(output.stdout, b"herdr 0.8.0\n");
    assert_eq!(output.exit_code, Some(0));

    connection
        .disconnect()
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn tracked_command_distinguishes_pre_dispatch_rejection() {
    let server = FakeSshServer::start(FakeSshOptions {
        exec: FakeExecBehavior::Reject,
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let options = options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1);
    let (connection, _events) =
        connect_with_trust(options, Arc::clone(&broker), HostKeyDecision::TrustOnce)
            .await
            .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let error = match connection
        .run_command_tracked("echo never-started", Duration::from_secs(1), 1024)
        .await
    {
        Err(error) => error,
        Ok(output) => unreachable!("rejected exec unexpectedly succeeded: {output:?}"),
    };
    assert!(!error.dispatched);

    server.shutdown().await;
}

#[tokio::test]
async fn tracked_command_marks_timeout_after_exec_ack_as_dispatched() {
    let server = FakeSshServer::start(FakeSshOptions {
        exec: FakeExecBehavior::ScriptedWithPersistent {
            script: Vec::new(),
            fallback: FakeExecResponse {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: 0,
            },
            persistent_prefixes: vec!["sleep".to_string()],
        },
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("fake server start: {error}"));
    let broker = new_broker();
    let options = options_for(server.addr().port(), PASSWORD, Arc::clone(&broker), 1);
    let (connection, _events) =
        connect_with_trust(options, Arc::clone(&broker), HostKeyDecision::TrustOnce)
            .await
            .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let error = match connection
        .run_command_tracked("sleep forever", Duration::from_millis(100), 1024)
        .await
    {
        Err(error) => error,
        Ok(output) => unreachable!("persistent exec unexpectedly completed: {output:?}"),
    };
    assert!(error.dispatched);
    assert_eq!(error.source, kodework_ssh::SshError::Timeout);

    server.shutdown().await;
}
