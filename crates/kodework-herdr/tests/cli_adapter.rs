//! Herdr CLI adapter tests with an injected fake remote executor.

use kodework_herdr::cli::{ExecOutput, HerdrClient, HerdrStatus, RemoteExecutor};
use kodework_herdr::{HerdrAgentStatus, HerdrError};
use std::time::Duration;

/// Scripted executor keyed by the command prefix.
struct FakeExecutor {
    responses: Vec<(String, ExecOutput)>,
    timeout: Option<HerdrError>,
}

#[async_trait::async_trait]
impl RemoteExecutor for FakeExecutor {
    async fn exec(&self, command: &str, _timeout: Duration) -> Result<ExecOutput, HerdrError> {
        if let Some(error) = &self.timeout {
            return Err(error.clone());
        }
        for (fragment, output) in &self.responses {
            if command.contains(fragment) {
                return Ok(output.clone());
            }
        }
        Ok(ExecOutput {
            stdout: Vec::new(),
            stderr: b"unhandled command".to_vec(),
            exit_code: 2,
        })
    }
}

fn ok(stdout: &str) -> ExecOutput {
    ExecOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        exit_code: 0,
    }
}

fn client(executor: FakeExecutor) -> HerdrClient {
    HerdrClient::new(Box::new(executor), Duration::from_secs(3))
}

const STATUS_JSON: &str = r#"{
  "server": {
    "running": true,
    "version": "0.9.0",
    "protocol_version": 20
  },
  "protocol_version": 20,
  "workspaces": [
    {
      "id": "ws-1",
      "name": "main",
      "tabs": [
        {
          "id": "tab-1",
          "name": "code",
          "panes": [
            {
              "id": "pane-1",
              "name": "shell",
              "agent": { "status": "working" }
            }
          ]
        }
      ]
    }
  ]
}"#;

const SCHEMA_JSON: &str =
    r#"{"protocol_version":20,"methods":{"workspace_list":true},"future":true}"#;

#[tokio::test]
async fn detect_returns_version() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr --version".into(),
            ok("herdr 0.9.0
"),
        )],
        timeout: None,
    };
    let version = client(executor)
        .detect()
        .await
        .unwrap_or_else(|error| unreachable!("detect: {error}"));
    assert!(version.contains("0.9.0"));
}

#[tokio::test]
async fn detect_reports_not_installed() {
    let executor = FakeExecutor {
        responses: vec![
            (
                "herdr --version".into(),
                ExecOutput {
                    stdout: Vec::new(),
                    stderr: b"herdr: command not found".to_vec(),
                    exit_code: 127,
                },
            ),
            (
                "HERDR_BIN".into(),
                ExecOutput {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    exit_code: 127,
                },
            ),
        ],
        timeout: None,
    };
    assert_eq!(
        client(executor).detect().await,
        Err(HerdrError::NotInstalled)
    );
}

#[tokio::test]
async fn status_parses_tolerantly_with_agent_states() {
    let executor = FakeExecutor {
        responses: vec![("herdr status --json".into(), ok(STATUS_JSON))],
        timeout: None,
    };
    let status: HerdrStatus = client(executor)
        .status()
        .await
        .unwrap_or_else(|error| unreachable!("status: {error}"));
    let server = status
        .server
        .as_ref()
        .unwrap_or_else(|| unreachable!("server"));
    assert_eq!(server.running, Some(true));
    assert_eq!(server.protocol_version, Some(20));
    let workspaces = status
        .workspaces
        .as_ref()
        .unwrap_or_else(|| unreachable!("ws"));
    assert_eq!(workspaces.len(), 1);
    let pane = workspaces[0]
        .tabs
        .as_ref()
        .and_then(|tabs| tabs.first())
        .and_then(|tab| tab.panes.as_ref())
        .and_then(|panes| panes.first())
        .unwrap_or_else(|| unreachable!("pane"));
    assert_eq!(pane.id.as_deref(), Some("pane-1"));
    let agent = pane.agent.as_ref().unwrap_or_else(|| unreachable!("agent"));
    assert_eq!(agent.domain_status(), HerdrAgentStatus::Working);
}

#[tokio::test]
async fn capabilities_parse_schema_with_future_fields() {
    let executor = FakeExecutor {
        responses: vec![("herdr api schema --json".into(), ok(SCHEMA_JSON))],
        timeout: None,
    };
    let capabilities = client(executor)
        .capabilities()
        .await
        .unwrap_or_else(|error| unreachable!("capabilities: {error}"));
    assert_eq!(capabilities.protocol_version, Some(20));
    assert!(capabilities.workspace_api);
    assert!(capabilities.agent_api);
}

#[tokio::test]
async fn protocol_mismatch_is_detected() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr api schema --json".into(),
            ok(r#"{"protocol_version":0}"#),
        )],
        timeout: None,
    };
    assert_eq!(
        client(executor).capabilities().await,
        Err(HerdrError::ProtocolMismatch)
    );
}

#[tokio::test]
async fn server_not_running_is_classified() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr status --json".into(),
            ExecOutput {
                stdout: Vec::new(),
                stderr: b"error: herdr server is not running".to_vec(),
                exit_code: 1,
            },
        )],
        timeout: None,
    };
    assert_eq!(
        client(executor).status().await,
        Err(HerdrError::ServerNotRunning)
    );
}

#[tokio::test]
async fn invalid_json_is_typed() {
    let executor = FakeExecutor {
        responses: vec![("herdr status --json".into(), ok("table output, not json"))],
        timeout: None,
    };
    assert_eq!(
        client(executor).status().await,
        Err(HerdrError::InvalidResponse)
    );
}

#[tokio::test]
async fn executor_timeout_propagates() {
    let executor = FakeExecutor {
        responses: Vec::new(),
        timeout: Some(HerdrError::Timeout),
    };
    assert_eq!(client(executor).status().await, Err(HerdrError::Timeout));
}

#[tokio::test]
async fn workspaces_list_round_trip() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr workspace list".into(),
            ok(
                r#"{"id":"cli:workspace:list","result":{"workspaces":[{"id":"ws-1","name":"main"},{"id":"ws-2","name":"agent"}]}}"#,
            ),
        )],
        timeout: None,
    };
    let workspaces = client(executor)
        .workspaces()
        .await
        .unwrap_or_else(|error| unreachable!("workspaces: {error}"));
    assert_eq!(workspaces.len(), 2);
    assert_eq!(workspaces[1].name.as_deref(), Some("agent"));
}

#[tokio::test]
async fn current_agent_list_envelope_and_field_names_are_supported() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr agent list".into(),
            ok(r#"{
              "id":"cli:agent:list",
              "result":{"agents":[{
                "terminal_id":"term-1",
                "name":"codex",
                "agent":"codex-cli",
                "agent_status":"working",
                "workspace_id":"ws-1",
                "tab_id":"tab-1",
                "pane_id":"pane-1",
                "focused":true,
                "revision":3
              }]}
            }"#),
        )],
        timeout: None,
    };
    let agents = client(executor)
        .agents()
        .await
        .unwrap_or_else(|error| unreachable!("agents: {error}"));
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].name.as_deref(), Some("codex"));
    assert_eq!(agents[0].kind.as_deref(), Some("codex-cli"));
    assert_eq!(agents[0].domain_status(), HerdrAgentStatus::Working);
}

#[tokio::test]
async fn common_user_install_path_is_used_when_exec_path_is_minimal() {
    let executor = FakeExecutor {
        responses: vec![
            ("-lc 'exec herdr --version'".into(), ok("herdr 0.8.0\n")),
            (
                "herdr --version".into(),
                ExecOutput {
                    stdout: Vec::new(),
                    stderr: b"herdr: command not found".to_vec(),
                    exit_code: 127,
                },
            ),
            ("$HOME/.cargo/bin/herdr".into(), ok("herdr 0.8.0\n")),
        ],
        timeout: None,
    };
    assert_eq!(client(executor).detect().await, Ok("herdr 0.8.0".into()));
}

#[tokio::test]
async fn conda_environment_install_is_discovered_for_noninteractive_ssh() {
    let executor = FakeExecutor {
        responses: vec![
            (
                "herdr --version".into(),
                ExecOutput {
                    stdout: Vec::new(),
                    stderr: b"herdr: command not found".to_vec(),
                    exit_code: 127,
                },
            ),
            (
                "$HOME\"/.conda/envs/*/bin/herdr".into(),
                ok("herdr 0.9.1\n"),
            ),
        ],
        timeout: None,
    };
    assert_eq!(client(executor).detect().await, Ok("herdr 0.9.1".into()));
}

#[tokio::test]
async fn login_shell_path_is_used_before_fixed_install_locations() {
    let executor = FakeExecutor {
        responses: vec![
            ("-lc 'exec herdr --version'".into(), ok("herdr 0.8.0\n")),
            (
                "herdr --version".into(),
                ExecOutput {
                    stdout: Vec::new(),
                    stderr: b"herdr: command not found".to_vec(),
                    exit_code: 127,
                },
            ),
        ],
        timeout: None,
    };
    assert_eq!(client(executor).detect().await, Ok("herdr 0.8.0".into()));
}

#[tokio::test]
async fn broken_herdr_install_is_not_misreported_as_missing() {
    let executor = FakeExecutor {
        responses: vec![(
            "herdr --version".into(),
            ExecOutput {
                stdout: Vec::new(),
                stderr: b"error while loading shared libraries: libexample.so".to_vec(),
                exit_code: 127,
            },
        )],
        timeout: None,
    };
    assert!(matches!(
        client(executor).detect().await,
        Err(HerdrError::CommandFailed { exit_code: 127, .. })
    ));
}
