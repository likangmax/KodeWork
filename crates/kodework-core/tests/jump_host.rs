//! Jump-host integration test: the real SSH connection is chained
//! through a bastion (fake SSH A bridging direct-tcpip to the target
//! fake SSH B), then a second SSH session runs over the tunnel.

use kodework_core::session::{SessionManager, SessionOutcome};
use kodework_domain::{Address, AddressId, AddressKind, Host, HostId, JumpHost, RuntimeKind};
use kodework_network::{CandidateResolver, ResolverPolicy};
use kodework_ssh::connection::{AuthMethod, ZeroizingVec};
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_testkit::fake_ssh::{FakeSshOptions, FakeSshServer};
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

/// Answers every pending host-key request with TrustAndSave.
async fn trust_all_pending(host_key: &Arc<HostKeyBroker>, expected: usize) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut answered = 0;
    while answered < expected {
        let requests = host_key.drain_requests();
        for request in requests {
            assert!(host_key.answer(request.request_id, HostKeyDecision::TrustAndSave));
            answered += 1;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "host key requests not answered in time ({answered}/{expected})"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Starts a target SSH server and a bastion that bridges direct-tcpip
/// traffic to it; returns (bastion_server, target_server).
async fn start_topology() -> (FakeSshServer, FakeSshServer) {
    let target = FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("target server: {error}"));
    let bastion = FakeSshServer::start(FakeSshOptions {
        jump_target: Some(target.addr()),
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("bastion server: {error}"));
    (bastion, target)
}

fn host_with_jump(bastion_port: u16, target_port: u16) -> Host {
    Host {
        id: HostId::new(),
        label: "jumped".into(),
        username: USER.into(),
        port: 22,
        auth_ref: None,
        auth_mode: kodework_domain::AuthenticationMode::Password,
        private_key_path: None,
        default_remote_path: "/".into(),
        jump: Some(JumpHost {
            hostname: "127.0.0.1".into(),
            port: bastion_port,
            username: USER.into(),
            auth_ref: None,
            auth_mode: kodework_domain::AuthenticationMode::Password,
            private_key_path: None,
        }),
        addresses: vec![Address {
            id: AddressId::new(),
            kind: AddressKind::Tailscale,
            hostname_or_ip: "127.0.0.1".into(),
            port: target_port,
            priority: 0,
            enabled: true,
        }],
        tailscale: None,
        default_runtime: RuntimeKind::PlainShell,
    }
}

#[tokio::test]
async fn connection_chains_through_jump_host() {
    let (bastion, target) = start_topology().await;
    let host = host_with_jump(bastion.addr().port(), target.addr().port());
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    // Two host keys: the bastion and the target (through the tunnel).
    trust_all_pending(&host_key, 2).await;
    let outcome = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { host_id, .. } if host_id == host.id
    ));

    // The terminal works over the chained transport.
    let (pane_id, channel_id) = manager
        .open_pane(host.id, 100, 30)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));
    let mut events = manager
        .subscribe(host.id, Some(channel_id))
        .unwrap_or_else(|| unreachable!("subscribe"));
    manager
        .send_input(host.id, pane_id, b"via-jump")
        .await
        .unwrap_or_else(|error| unreachable!("send_input: {error}"));
    let mut echoed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, events.recv()).await {
            Ok(Some(kodework_ssh::handler::SessionEvent::Data { bytes, .. })) => {
                echoed.extend_from_slice(&bytes);
                if echoed.windows(10).any(|window| window == b"via-jump\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        echoed.windows(10).any(|window| window == b"via-jump\r\n"),
        "PTY echo must survive the jump chain (got {:?})",
        String::from_utf8_lossy(&echoed)
    );

    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));
    bastion.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn jump_host_key_change_is_hard_failure() {
    // First connection records the bastion key; a second bastion with a
    // different key must be rejected even before the target is reached.
    let (bastion, target) = start_topology().await;
    let host = host_with_jump(bastion.addr().port(), target.addr().port());
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    trust_all_pending(&host_key, 2).await;
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    manager
        .disconnect(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("disconnect: {error}"));

    // Restart the bastion on the same port with a different host key.
    let bastion_port = bastion.addr().port();
    bastion.shutdown().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let new_key = kodework_testkit::fake_ssh::FakeSshServer::start_with_new_key_on_port(
        FakeSshOptions {
            jump_target: Some(target.addr()),
            ..FakeSshOptions::default()
        },
        bastion_port,
    )
    .await
    .unwrap_or_else(|error| unreachable!("restart bastion: {error}"));

    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    // Only the target key is pending; the bastion key mismatch must be a
    // hard failure (no fallback, no silent acceptance).
    let mut requests = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        requests = host_key.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(requests.is_empty(), "bastion key change must not prompt");
    let error = match outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
    {
        Err(error) => error,
        Ok(_) => unreachable!("changed bastion key must fail the connection"),
    };
    assert!(error.contains("host key changed"), "got: {error}");

    new_key.shutdown().await;
    target.shutdown().await;
}
