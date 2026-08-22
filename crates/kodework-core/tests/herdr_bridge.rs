//! Herdr socket bridge integration tests: socket probing, socat
//! detection and bridge startup over the fake SSH server.

use kodework_core::session::SessionManager;
use kodework_domain::{Address, AddressId, AddressKind, Host, HostId, RuntimeKind};
use kodework_network::{CandidateResolver, ResolverPolicy};
use kodework_ssh::connection::{AuthMethod, ZeroizingVec};
use kodework_ssh::host_key::{HostKeyBroker, HostKeyDecision, MemoryKnownHosts};
use kodework_testkit::fake_ssh::{
    FakeExecBehavior, FakeExecResponse, FakeSshOptions, FakeSshServer,
};
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

fn host_with(port: u16) -> Host {
    Host {
        id: HostId::new(),
        label: "herdr-host".into(),
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
        default_runtime: RuntimeKind::Herdr,
    }
}

fn ok() -> FakeExecResponse {
    FakeExecResponse {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: 0,
    }
}

#[tokio::test]
async fn bridge_probes_socket_and_starts_socat() {
    let server = FakeSshServer::start(FakeSshOptions {
        exec: FakeExecBehavior::Scripted {
            script: vec![
                (
                    "printf".into(),
                    FakeExecResponse {
                        stdout: b"/home/tester/.herdr/run/herdr.sock\n".to_vec(),
                        stderr: Vec::new(),
                        exit_code: 0,
                    },
                ),
                (
                    "command -v socat".into(),
                    FakeExecResponse {
                        stdout: b"yes\n".to_vec(),
                        stderr: Vec::new(),
                        exit_code: 0,
                    },
                ),
                (
                    "exec socat".into(),
                    FakeExecResponse {
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        exit_code: 0,
                    },
                ),
            ],
            fallback: ok(),
        },
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("server: {error}"));
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
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let info = manager
        .herdr_bridge(host.id, 0)
        .await
        .unwrap_or_else(|error| unreachable!("bridge: {error}"));
    assert_eq!(info.remote_socket, "/home/tester/.herdr/run/herdr.sock");
    assert!(info.tunnel.local_addr.starts_with("127.0.0.1:"));

    manager
        .herdr_bridge_stop(host.id, info.remote_port, Some(info.remote_pid))
        .await
        .unwrap_or_else(|error| unreachable!("bridge stop: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn bridge_without_socket_reports_clear_error() {
    let server = FakeSshServer::start(FakeSshOptions {
        exec: FakeExecBehavior::Scripted {
            script: vec![(
                "printf".into(),
                FakeExecResponse {
                    stdout: b"\n".to_vec(),
                    stderr: Vec::new(),
                    exit_code: 0,
                },
            )],
            fallback: ok(),
        },
        ..FakeSshOptions::default()
    })
    .await
    .unwrap_or_else(|error| unreachable!("server: {error}"));
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
    outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));

    let error = match manager.herdr_bridge(host.id, 0).await {
        Err(error) => error,
        Ok(_) => unreachable!("no socket must fail"),
    };
    assert!(error.contains("herdr socket"), "got: {error}");
    server.shutdown().await;
}
