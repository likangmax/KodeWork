//! SSH local port forwarding integration tests over the fake SSH
//! server (which echoes direct-tcpip channel data verbatim).

use kodework_core::session::{SessionManager, SessionOutcome};
use kodework_core::tunnel::TunnelState;
use kodework_domain::{Address, AddressId, AddressKind, Host, HostId, RuntimeKind};
use kodework_network::{CandidateResolver, ResolverPolicy};
use kodework_ssh::connection::{AuthMethod, ZeroizingVec};
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_testkit::fake_ssh::{FakeSshOptions, FakeSshServer};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

fn host_with(port: u16) -> Host {
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
        addresses: vec![Address {
            id: AddressId::new(),
            kind: AddressKind::Tailscale,
            hostname_or_ip: "127.0.0.1".into(),
            port,
            priority: 0,
            enabled: true,
        }],
        tailscale: None,
        default_runtime: RuntimeKind::PlainShell,
    }
}

async fn connect_manager() -> (FakeSshServer, SessionManager, Host) {
    let server = kodework_testkit::fake_ssh::FakeSshServer::start(FakeSshOptions::default())
        .await
        .unwrap_or_else(|error| unreachable!("fake server: {error}"));
    let host = host_with(server.addr().port());
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(Arc::clone(&host_key), resolver, 512);
    let outcome_task = tokio::spawn({
        let manager = manager.clone();
        let host = host.clone();
        let auth = auth();
        async move { manager.connect(&host, auth).await }
    });
    let mut requests = Vec::new();
    for _ in 0..200 {
        requests = host_key.drain_requests();
        if !requests.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(requests.len(), 1);
    assert!(host_key.answer(requests[0].request_id, HostKeyDecision::TrustOnce));
    let outcome = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { host_id, .. } if host_id == host.id
    ));
    (server, manager, host)
}

/// Opens a tunnel to a fixed remote port and round-trips a payload
/// through the fake echo service.
async fn round_trip(manager: &SessionManager, host_id: HostId, payload: &[u8]) {
    let info = manager
        .open_tunnel(host_id, 0, "127.0.0.1", 8080)
        .await
        .unwrap_or_else(|error| unreachable!("open_tunnel: {error}"));
    assert_eq!(info.state, TunnelState::Listening);
    assert!(
        info.local_addr.starts_with("127.0.0.1:"),
        "got {}",
        info.local_addr
    );

    let addr = info.local_addr.clone();
    let mut stream = tokio::net::TcpStream::connect(&addr)
        .await
        .unwrap_or_else(|error| unreachable!("connect local: {error}"));
    stream
        .write_all(payload)
        .await
        .unwrap_or_else(|error| unreachable!("write: {error}"));
    let mut buf = vec![0u8; payload.len()];
    stream
        .read_exact(&mut buf)
        .await
        .unwrap_or_else(|error| unreachable!("read echo: {error}"));
    assert_eq!(buf, payload, "echo must be byte-exact");

    // close and verify the listener is gone
    manager
        .close_tunnel(info.id)
        .await
        .unwrap_or_else(|error| unreachable!("close_tunnel: {error}"));
    let listed = manager.list_tunnels();
    let closed = listed
        .iter()
        .find(|tunnel| tunnel.id == info.id)
        .unwrap_or_else(|| unreachable!("tunnel must stay listed"));
    assert_eq!(closed.state, TunnelState::Closed);
    let refused = tokio::net::TcpStream::connect(&addr).await;
    assert!(
        refused.is_err(),
        "listener must be closed after close_tunnel"
    );
}

#[tokio::test]
async fn tunnel_round_trips_and_closes() {
    let (_server, manager, host) = connect_manager().await;
    round_trip(&manager, host.id, b"ping from tunnel").await;
}

#[tokio::test]
async fn tunnel_handles_concurrent_connections() {
    let (_server, manager, host) = connect_manager().await;
    let info = manager
        .open_tunnel(host.id, 0, "127.0.0.1", 8080)
        .await
        .unwrap_or_else(|error| unreachable!("open_tunnel: {error}"));
    let addr = info.local_addr.clone();

    let mut handles = Vec::new();
    for round in 0..4u32 {
        let addr = addr.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(&addr)
                .await
                .unwrap_or_else(|error| unreachable!("connect: {error}"));
            let payload = format!("conn-{round}").into_bytes();
            stream
                .write_all(&payload)
                .await
                .unwrap_or_else(|error| unreachable!("write: {error}"));
            let mut buf = vec![0u8; payload.len()];
            stream
                .read_exact(&mut buf)
                .await
                .unwrap_or_else(|error| unreachable!("read: {error}"));
            assert_eq!(buf, payload);
        }));
    }
    for handle in handles {
        handle
            .await
            .unwrap_or_else(|error| unreachable!("join: {error}"));
    }

    // The server-side proxy tasks drain asynchronously; poll until the
    // live connection count reaches zero.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let listed = manager.list_tunnels();
        let tunnel = listed
            .iter()
            .find(|tunnel| tunnel.id == info.id)
            .unwrap_or_else(|| unreachable!("tunnel"));
        if tunnel.active_connections == 0 || tokio::time::Instant::now() >= deadline {
            assert_eq!(tunnel.active_connections, 0, "all connections drained");
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    manager
        .close_tunnel(info.id)
        .await
        .unwrap_or_else(|error| unreachable!("close: {error}"));
}

#[tokio::test]
async fn tunnel_fails_without_connection() {
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(host_key, resolver, 512);
    let host = host_with(1);
    let error = match manager.open_tunnel(host.id, 0, "127.0.0.1", 8080).await {
        Err(error) => error,
        Ok(_) => unreachable!("must fail without a connection"),
    };
    assert!(
        error.contains("not connected") || error.contains("no session for host"),
        "got: {error}"
    );
}

#[tokio::test]
async fn tunnel_rejects_bad_arguments() {
    let (_server, manager, host) = connect_manager().await;
    let error = match manager.open_tunnel(host.id, 0, " ", 0).await {
        Err(error) => error,
        Ok(_) => unreachable!("empty remote must error"),
    };
    assert!(error.contains("remote host"), "got: {error}");
}
