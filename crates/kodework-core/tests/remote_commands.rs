//! Remote command integration tests: run_remote, tmux session management
//! and the Herdr CLI adapter, all over the fake SSH server.

use kodework_core::session::{SessionManager, SessionOutcome};
use kodework_domain::{
    Action, ActionId, ActionMode, Address, AddressId, AddressKind, ConfirmationPolicy, DangerLevel,
    Host, HostId, ProjectId, RuntimeKind,
};
use kodework_herdr::cli::{parse_agent_list, parse_status, parse_workspace_list};
use kodework_herdr::HerdrAgentStatus;
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
        default_runtime: RuntimeKind::Herdr,
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

/// Connects to a fake server whose exec channel follows `script`; returns
/// the connected manager.
async fn connect_with(
    script: Vec<(String, FakeExecResponse)>,
    fallback: FakeExecResponse,
) -> (FakeSshServer, Arc<HostKeyBroker>, SessionManager, Host) {
    let server = FakeSshServer::start(FakeSshOptions {
        exec: FakeExecBehavior::Scripted { script, fallback },
        ..FakeSshOptions::default()
    })
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
    trust_pending(&host_key, HostKeyDecision::TrustOnce).await;
    let outcome = outcome_task
        .await
        .unwrap_or_else(|error| unreachable!("join: {error}"))
        .unwrap_or_else(|error| unreachable!("connect: {error}"));
    assert!(matches!(
        outcome,
        SessionOutcome::Connected { host_id, .. } if host_id == host.id
    ));
    (server, host_key, manager, host)
}

/// Unwraps a `Result` into its `Err(String)` or fails the test.
fn err_of<T>(result: Result<T, String>) -> String {
    match result {
        Err(error) => error,
        Ok(_) => unreachable!("expected an error"),
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
async fn run_remote_captures_output_and_exit_code() {
    let (server, _host_key, manager, host) = connect_with(
        vec![(
            "printf".into(),
            FakeExecResponse {
                stdout: b"hello remote\n".to_vec(),
                stderr: b"ignored\n".to_vec(),
                exit_code: 7,
            },
        )],
        ok(),
    )
    .await;

    let output = manager
        .run_remote(host.id, "printf 'hello remote\n'")
        .await
        .unwrap_or_else(|error| unreachable!("run_remote: {error}"));
    assert_eq!(output.stdout, b"hello remote\n");
    assert_eq!(output.exit_code, Some(7));
    assert!(!output.stdout_truncated);
    server.shutdown().await;
}

#[tokio::test]
async fn run_remote_fails_fast_when_disconnected() {
    let host = host_with(1);
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(host_key, resolver, 512);
    let error = err_of(manager.run_remote(host.id, "echo hi").await);
    assert!(
        error.contains("no session for host") || error.contains("not connected"),
        "got: {error}"
    );
}

#[tokio::test]
async fn tmux_lists_creates_and_kills_sessions() {
    let tmux_ls = FakeExecResponse {
        stdout: b"dev\t3\t1\t2025-01-02 10:00:00\nops\t1\t0\t2025-01-03 09:30:00\n".to_vec(),
        stderr: Vec::new(),
        exit_code: 0,
    };
    let (server, _host_key, manager, host) = connect_with(
        vec![
            ("tmux ls".into(), tmux_ls),
            ("tmux new-session -d -s dev".into(), {
                let mut response = ok();
                response.stderr = b"duplicate session: dev\n".to_vec();
                response.exit_code = 1;
                response
            }),
            ("tmux kill-session -t ops".into(), ok()),
        ],
        ok(),
    )
    .await;

    let sessions = manager
        .tmux_list(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("tmux_list: {error}"));
    assert_eq!(sessions.len(), 2, "two tmux sessions");
    assert_eq!(sessions[0].name, "dev");
    assert_eq!(sessions[0].windows, 3);
    assert_eq!(sessions[0].attached, 1);
    assert_eq!(sessions[1].name, "ops");

    let error = err_of(manager.tmux_new(host.id, "dev").await);
    assert!(error.contains("already exists"), "got: {error}");

    manager
        .tmux_kill(host.id, "ops")
        .await
        .unwrap_or_else(|error| unreachable!("tmux_kill: {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn tmux_name_validation_blocks_injection() {
    let host_key = broker();
    let resolver = CandidateResolver::new(Vec::new(), ResolverPolicy::default());
    let manager = SessionManager::new(host_key, resolver, 512);
    let host = host_with(1);

    let error = err_of(manager.tmux_new(host.id, "a; rm -rf /").await);
    assert!(error.contains("may only contain"), "got: {error}");

    let error = err_of(manager.tmux_new(host.id, "").await);
    assert!(error.contains("1..=64"), "got: {error}");
}

#[tokio::test]
async fn herdr_detect_and_agent_list_over_ssh() {
    let agent_json = r#"{"id":"cli:agent:list","result":{"agents":[{"terminal_id":"term-1","name":"reviewer","agent":"codex","agent_status":"working","workspace_id":"w1","tab_id":"tab-1","pane_id":"w1:p3","focused":true,"revision":1},{"terminal_id":"term-2","name":"coder","agent":"claude","agent_status":"idle","workspace_id":"w1","tab_id":"tab-1","pane_id":"w1:p4","focused":false,"revision":1}]}}"#;
    let (server, _host_key, manager, host) = connect_with(
        vec![
            (
                "herdr --version".into(),
                FakeExecResponse {
                    stdout: b"herdr 0.8.0\n".to_vec(),
                    stderr: Vec::new(),
                    exit_code: 0,
                },
            ),
            (
                "herdr agent list".into(),
                FakeExecResponse {
                    stdout: agent_json.as_bytes().to_vec(),
                    stderr: Vec::new(),
                    exit_code: 0,
                },
            ),
        ],
        ok(),
    )
    .await;

    let client = manager.herdr_client(host.id);
    let version = client
        .detect()
        .await
        .unwrap_or_else(|error| unreachable!("detect: {error}"));
    assert_eq!(version, "herdr 0.8.0");

    let agents = client
        .agents()
        .await
        .unwrap_or_else(|error| unreachable!("agents: {error}"));
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].name.as_deref(), Some("reviewer"));
    assert_eq!(agents[0].kind.as_deref(), Some("codex"));
    assert_eq!(agents[0].domain_status(), HerdrAgentStatus::Working);
    assert_eq!(agents[1].domain_status(), HerdrAgentStatus::Idle);
    assert_eq!(agents[1].pane_id.as_deref(), Some("w1:p4"));
    server.shutdown().await;
}

#[tokio::test]
async fn herdr_missing_binary_maps_to_not_installed() {
    let missing = FakeExecResponse {
        stdout: Vec::new(),
        stderr: b"herdr: command not found\n".to_vec(),
        exit_code: 127,
    };
    let (server, _host_key, manager, host) = connect_with(
        vec![
            ("herdr --version".into(), missing.clone()),
            ("if [ -n \"$SHELL\"".into(), missing.clone()),
            ("HERDR_BIN=".into(), missing),
        ],
        ok(),
    )
    .await;

    let client = manager.herdr_client(host.id);
    let error = match client.detect().await {
        Err(error) => error,
        Ok(_) => unreachable!("missing herdr must error"),
    };
    assert_eq!(error, kodework_herdr::HerdrError::NotInstalled);
    server.shutdown().await;
}

#[tokio::test]
async fn herdr_attach_writes_to_the_pty() {
    let (server, _host_key, manager, host) = connect_with(Vec::new(), ok()).await;
    let (_pane_id, channel_id) = manager
        .open_pane(host.id, 120, 40)
        .await
        .unwrap_or_else(|error| unreachable!("open_pane: {error}"));
    let mut events = manager
        .subscribe(host.id, Some(channel_id))
        .unwrap_or_else(|| unreachable!("subscribe"));
    manager
        .herdr_attach(host.id)
        .await
        .unwrap_or_else(|error| unreachable!("herdr_attach: {error}"));

    let mut echoed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        match tokio::time::timeout(deadline - now, events.recv()).await {
            Ok(Some(kodework_ssh::handler::SessionEvent::Data { bytes, .. })) => {
                echoed.extend_from_slice(&bytes);
                if echoed.windows(7).any(|window| window == b"herdr\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        echoed.windows(5).any(|window| window == b"herdr"),
        "herdr attach must reach the PTY"
    );
    server.shutdown().await;
}

#[test]
fn herdr_parsers_accept_bare_and_enveloped_json() {
    let bare = parse_agent_list(r#"[{"name":"a","kind":"codex","status":"blocked"}]"#)
        .unwrap_or_else(|error| unreachable!("bare parse: {error}"));
    assert_eq!(bare[0].domain_status(), HerdrAgentStatus::Blocked);

    let enveloped = parse_agent_list(r#"{"result":[{"name":"b","status":"done"}],"error":null}"#)
        .unwrap_or_else(|error| unreachable!("envelope parse: {error}"));
    assert_eq!(enveloped[0].domain_status(), HerdrAgentStatus::Done);

    let with_error = parse_agent_list(r#"{"result":null,"error":"server down"}"#);
    assert!(with_error.is_err(), "explicit errors must surface");

    let garbage = parse_agent_list("not json at all");
    assert_eq!(garbage, Err(kodework_herdr::HerdrError::InvalidResponse));

    assert!(parse_status(r#"{"server":{"running":true}}"#).is_ok());
    assert!(parse_workspace_list(r#"{"result":[],"error":null}"#).is_ok());
}

#[tokio::test]
async fn background_action_is_detached_into_tmux_and_returns_reference() {
    let (server, _host_key, manager, host) = connect_with(
        vec![("tmux new-session -d -s kodework-run-".into(), ok())],
        ok(),
    )
    .await;
    let action = Action {
        id: ActionId::new(),
        project_id: ProjectId::new(),
        name: "build".into(),
        command: "cargo build --release".into(),
        mode: ActionMode::Background,
        cwd: Some("~/workspace/project".into()),
        timeout_ms: None,
        danger_level: DangerLevel::Safe,
        confirmation: ConfirmationPolicy::Never,
        env: std::collections::BTreeMap::new(),
    };
    let outcome = manager
        .run_action(host.id, &action, false)
        .await
        .unwrap_or_else(|error| unreachable!("background action: {error}"));
    assert_eq!(
        outcome.disposition,
        kodework_core::session::RunDisposition::BackgroundStarted
    );
    assert_eq!(outcome.exit_code, None);
    assert!(outcome
        .remote_session_ref
        .as_deref()
        .is_some_and(|value| value.starts_with("tmux:kodework-run-")));
    server.shutdown().await;
}
