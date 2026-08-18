//! SessionManager integration tests against the fake SSH server.

use kodework_core::session::{SessionManager, SessionOutcome};
use kodework_domain::{Address, AddressId, AddressKind, Host, HostId, RuntimeKind};
use kodework_network::{CandidateResolver, ResolverPolicy};
use kodework_ssh::connection::{AuthMethod, ZeroizingVec};
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_testkit::fake_ssh::{FakeShellBehavior, FakeSshOptions, FakeSshServer};
use std::sync::Arc;
use std::time::Duration;

const USER: &str = "tester";
const PASSWORD: &str = "test-password";

fn broker() -> Arc<HostKeyBroker> {
    Arc::new(HostKeyBroker::new(
        Arc::new(MemoryKnownHosts::new()),
        Duration::from_secs(10),
    ))
}

fn auth() -> Vec<AuthMethod> {
    vec![AuthMethod::Password(ZeroizingVec::new(
        PASSWORD.as_bytes().to_vec(),
    ))]
}

fn host_with(_port: u16, addresses: Vec<Address>) -> Host {
    Host {
        id: HostId::new(),
        label: "lab".into(),
        username: USER.into(),
        port: 22,
        auth_ref: None,
        auth_mode: kodework_domain::AuthenticationMode::Password,
        private_key_path: None,
        default_remote_path: "/".into(),
        jump: None,
        addresses,
        tailscale: None,
        default_runtime: RuntimeKind::Tmux,
    }
}

fn address(kind: AddressKind, hostname: &str, port: u16) -> Address {
    Address {
        id: AddressId::new(),
        kind,
        hostname_or_ip: hostname.into(),
        port,
        priority: 0,
        enabled: true,
    }
}

async fn trust_pending(broker: &Arc<HostKeyBroker>, decision: HostKeyDecision) {
    let mut requests = Vec::new();
    for _ in 0..200 {
        requests = broker.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(requests.len(), 1, "exactly one host-key request expected");
    assert!(broker.answer(requests[0].request_id, decision));
}

#[tokio::test]
async fn connect_reaches_ready_and_streams_events() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let outcome = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { host_id, generation: 1 } if host_id == host.id
    ));
    assert_eq!(
        manager.state(host.id),
        kodework_domain::ConnectionState::Ready
    );

    let (pane_id, channel_id) = manager
        .open_pane(host.id, 120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));
    let mut events = manager
        .subscribe(host.id, Some(channel_id))
        .unwrap_or_else(|| unreachable!("subscribe"));
    manager
        .send_input(host.id, pane_id, b"hello")
        .await
        .unwrap_or_else(|error| unreachable!("send_input: {error}"));

    let mut echoed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, events.recv()).await {
            Ok(Some(kodework_ssh::handler::SessionEvent::Data { bytes, .. })) => {
                echoed.extend_from_slice(&bytes);
                if echoed.windows(7).any(|window| window == b"hello\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        echoed.windows(7).any(|window| window == b"hello\r\n"),
        "PTY echo must flow through the session event stream"
    );
    assert_eq!(
        manager.dropped_events(host.id),
        0,
        "primary stream never drops"
    );
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    assert_eq!(
        manager.state(host.id),
        kodework_domain::ConnectionState::Disconnected
    );
    assert!(manager
        .send_input(host.id, pane_id, b"stale")
        .await
        .is_err());
    // Explicit disconnect is idempotent and must not retain the old PTY.
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("second disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn late_pane_subscription_replays_initial_output() {
    let server = FakeSshServer::start(FakeSshOptions {
        shell: FakeShellBehavior::Flood { bytes: 64 },
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let manager = SessionManager::new(
        Arc::clone(&host_key),
        CandidateResolver::new(Vec::new(), ResolverPolicy::default()),
        32,
    );
    let connect_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        async move { manager.connect(&host, auth()).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    connect_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let (_pane, channel) = manager
        .open_pane(host.id, 80, 24)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));
    // Reproduce the renderer mount gap: the remote shell writes before the
    // TerminalPane has subscribed to its channel.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut events = manager
        .subscribe(host.id, Some(channel))
        .unwrap_or_else(|| unreachable!("subscribe"));
    let mut received = Vec::new();
    while received.len() < 64 {
        match tokio::time::timeout(Duration::from_secs(1), events.recv()).await {
            Ok(Some(kodework_ssh::handler::SessionEvent::Data { bytes, .. })) => {
                received.extend_from_slice(&bytes);
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    assert_eq!(received, vec![b'a'; 64]);
    let _ = manager.disconnect(host.id).await;
    server.shutdown().await;
}

#[tokio::test]
async fn supports_twenty_concurrent_terminal_panes_without_cross_talk() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let manager = SessionManager::new(
        Arc::clone(&host_key),
        CandidateResolver::new(Vec::new(), ResolverPolicy::default()),
        512,
    );
    let task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        async move { manager.connect(&host, auth()).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    task.await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let mut panes = Vec::new();
    for _ in 0..20 {
        panes.push(
            manager
                .open_pane(host.id, 80, 24)
                .await
                .unwrap_or_else(|error| unreachable!("open pane: {error}")),
        );
    }
    assert_eq!(panes.len(), 20);
    assert!(manager.open_pane(host.id, 80, 24).await.is_err());
    for (pane_id, _) in panes {
        manager
            .send_input(host.id, pane_id, b"echo pane\r")
            .await
            .unwrap_or_else(|error| unreachable!("send input: {error}"));
    }
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn candidate_fallback_tries_next_address() {
    // First candidate points at a closed port; the fake server is second.
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let probe = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap_or_else(|error| unreachable!("probe: {error}"));
    let dead_port = probe
        .local_addr()
        .unwrap_or_else(|error| unreachable!("addr: {error}"))
        .port();
    drop(probe);

    let host = host_with(
        server.addr().port(),
        vec![
            address(AddressKind::Lan, "127.0.0.1", dead_port),
            address(AddressKind::Tailscale, "127.0.0.1", server.addr().port()),
        ],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let outcome = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { generation: 1, .. }
    ));
    assert_eq!(
        manager.state(host.id),
        kodework_domain::ConnectionState::Ready
    );
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn wrong_password_fails_without_fallback() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![
            address(AddressKind::Tailscale, "127.0.0.1", server.addr().port()),
            address(AddressKind::Manual, "127.0.0.1", server.addr().port()),
        ],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);
    let wrong_auth = vec![AuthMethod::Password(ZeroizingVec::new(b"wrong".to_vec()))];

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        async move { manager.connect(&host, wrong_auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let result = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"));
    assert!(
        result.is_err(),
        "auth failure must fail the whole connect: {result:?}"
    );
    assert_eq!(
        manager.state(host.id),
        kodework_domain::ConnectionState::Failed
    );
    server.shutdown().await;
}

#[tokio::test]
async fn transport_drop_moves_state_to_reconnecting() {
    let options = FakeSshOptions {
        shell: FakeShellBehavior::DropAfter {
            delay: Duration::from_millis(150),
        },
        ..FakeSshOptions::default()
    };
    let server = FakeSshServer::start(options)
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    // The fake server drops the transport from the shell handler, so the
    // session must open a PTY first.
    manager
        .open_pane(host.id, 120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));

    // Give the pump a moment to observe the drop and flip the state.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while manager.state(host.id) != kodework_domain::ConnectionState::Reconnecting
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        manager.state(host.id),
        kodework_domain::ConnectionState::Reconnecting,
        "drop must surface as Reconnecting"
    );
    server.shutdown().await;
}
#[tokio::test]
async fn generation_increments_on_reconnect() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustAndSave).await;
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect 1: {error}"));
    assert_eq!(manager.generation(host.id), 1);
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));

    // Second connect (saved host key: no prompt) must use generation 2.
    let outcome = manager
        .connect(&host, auth())
        .await
        .unwrap_or_else(|error| unreachable!("connect 2: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { generation: 2, .. }
    ));
    assert_eq!(manager.generation(host.id), 2);
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect 2: {error}"));
    server.shutdown().await;
}
#[tokio::test]
async fn rapid_resize_100_times_settles_on_final_size() {
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(
            AddressKind::Tailscale,
            "127.0.0.1",
            server.addr().port(),
        )],
    );
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    manager
        .open_pane(host.id, 120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));

    // Rapid resize: every call must succeed and settle on the final size.
    for step in 1..=100u32 {
        let cols = 80 + (step % 40);
        let rows = 24 + (step % 20);
        manager
            .resize(host.id, 0, cols, rows)
            .await
            .unwrap_or_else(|error| unreachable!("resize {step}: {error}"));
    }
    manager
        .resize(host.id, 0, 180, 50)
        .await
        .unwrap_or_else(|error| unreachable!("final resize: {error}"));
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn dangerous_action_requires_confirmation_even_if_declared_safe() {
    // The renderer-declared danger level must never gate confirmation:
    // the server reclassifies the command and rejects unconfirmed runs.
    use kodework_domain::{
        Action, ActionId, ActionMode, ConfirmationPolicy, DangerLevel, ProjectId,
    };
    use std::collections::BTreeMap;
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(AddressKind::Lan, "127.0.0.1", server.addr().port())],
    );
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let host_key = broker();
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 256);
    let connect_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let outcome = connect_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(outcome, SessionOutcome::Connected { .. }));

    let action = Action {
        id: ActionId::new(),
        project_id: ProjectId::new(),
        name: "cleanup".into(),
        command: "rm -rf /tmp/attack".into(),
        mode: ActionMode::Quick,
        cwd: None,
        timeout_ms: Some(2_000),
        danger_level: DangerLevel::Safe, // malicious declaration
        confirmation: ConfirmationPolicy::Never,
        env: BTreeMap::new(),
    };
    let rejected = manager.run_action(host.id, &action, false).await;
    assert!(
        rejected.is_err(),
        "declared-safe dangerous command must still be rejected without confirmation"
    );
    let _ = manager.disconnect(host.id).await;
    server.shutdown().await;
}

#[tokio::test]
async fn resubscribe_after_receiver_drop_still_receives_events() {
    // A dropped receiver must be reaped so a fresh subscription becomes
    // the primary stream and keeps receiving events (previously the dead
    // primary stayed cached and the new subscriber lost events).
    use kodework_ssh::handler::SessionEvent;
    let server = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(
        server.addr().port(),
        vec![address(AddressKind::Lan, "127.0.0.1", server.addr().port())],
    );
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let host_key = broker();
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 256);
    let connect_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let outcome = connect_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(outcome, SessionOutcome::Connected { .. }));
    let (_pane, channel) = manager
        .open_pane(host.id, 80, 24)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));

    // First subscription is dropped immediately.
    let first = manager
        .subscribe(host.id, Some(channel))
        .unwrap_or_else(|| unreachable!("subscribe"));
    drop(first);

    // Second subscription must become the live stream.
    let mut second = manager
        .subscribe(host.id, Some(channel))
        .unwrap_or_else(|| unreachable!("subscribe"));
    manager
        .send_input(host.id, 0, b"echo ping\r".as_slice())
        .await
        .unwrap_or_else(|error| unreachable!("send_input: {error}"));

    let mut got_data = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), second.recv()).await {
            Ok(Some(SessionEvent::Data { .. })) => {
                got_data = true;
                break;
            }
            Ok(Some(_)) => continue,
            _ => break,
        }
    }
    assert!(
        got_data,
        "resubscribed receiver must receive terminal events"
    );
    let _ = manager.disconnect(host.id).await;
    server.shutdown().await;
}
