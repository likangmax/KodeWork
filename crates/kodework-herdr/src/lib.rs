#![forbid(unsafe_code)]

pub mod cli;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HerdrAgentStatus {
    Unknown,
    Idle,
    Working,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HerdrCapabilities {
    pub protocol_version: Option<u32>,
    pub workspace_api: bool,
    pub pane_api: bool,
    pub agent_api: bool,
    pub terminal_control: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HerdrError {
    #[error("herdr is not installed on the remote host")]
    NotInstalled,
    #[error("herdr protocol mismatch")]
    ProtocolMismatch,
    #[error("herdr server is not running")]
    ServerNotRunning,
    #[error("herdr command timed out")]
    Timeout,
    #[error("herdr response is invalid JSON")]
    InvalidResponse,
    #[error("herdr command failed with code {exit_code}: {stderr}")]
    CommandFailed { exit_code: i32, stderr: String },
    #[error("remote executor error: {0}")]
    Executor(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrCommand {
    pub argv: Vec<String>,
    pub timeout_ms: u64,
}

impl HerdrCommand {
    #[must_use]
    pub fn status() -> Self {
        Self {
            argv: vec!["herdr".into(), "status".into()],
            timeout_ms: 5_000,
        }
    }
    #[must_use]
    pub fn api_schema() -> Self {
        Self {
            argv: vec![
                "herdr".into(),
                "api".into(),
                "schema".into(),
                "--json".into(),
            ],
            timeout_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HerdrSchemaEnvelope {
    #[serde(default)]
    pub protocol_version: Option<u32>,
}

pub fn parse_schema(json: &str) -> Result<HerdrSchemaEnvelope, HerdrError> {
    serde_json::from_str(json).map_err(|_| HerdrError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_uses_argv_not_shell_string() {
        let command = HerdrCommand::api_schema();
        assert_eq!(command.argv, vec!["herdr", "api", "schema", "--json"]);
    }

    #[test]
    fn schema_parser_is_tolerant() {
        assert!(parse_schema(r#"{"protocol_version":20,"future":true}"#).is_ok());
        assert_eq!(parse_schema("bad"), Err(HerdrError::InvalidResponse));
    }
}
