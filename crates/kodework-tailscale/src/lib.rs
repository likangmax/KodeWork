#![forbid(unsafe_code)]

pub mod cli;
pub mod provider;
pub mod runtime;

use kodework_domain::{Address, AddressId, AddressKind, TailscaleMode};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TailscaleStatus {
    #[serde(rename = "BackendState", default)]
    pub backend_state: Option<String>,
    #[serde(rename = "Self", default)]
    pub self_node: Option<TailscaleNode>,
    #[serde(rename = "Peer", default, deserialize_with = "null_as_default")]
    pub peers: BTreeMap<String, TailscaleNode>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TailscaleNode {
    #[serde(rename = "HostName", default)]
    pub host_name: Option<String>,
    #[serde(rename = "DNSName", default)]
    pub dns_name: Option<String>,
    #[serde(rename = "Online", default)]
    pub online: Option<bool>,
    #[serde(rename = "TailscaleIPs", default, deserialize_with = "null_as_default")]
    pub tailscale_ips: Vec<String>,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    SystemDaemon,
    EmbeddedUserspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPlan {
    pub mode: TailscaleMode,
    pub program: Option<String>,
    pub args: Vec<String>,
    pub requires_auth_key_injection: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TailscaleError {
    #[error("tailscale status output is invalid JSON")]
    InvalidJson,
    #[error("embedded userspace backend requires an explicit private state directory")]
    MissingStateDirectory,
    #[error("tailscale state path must be absolute")]
    InvalidStatePath,
    #[error("failed to spawn tailscale: {0}")]
    Spawn(String),
    #[error("failed to read tailscale process output: {0}")]
    OutputRead(String),
    #[error("tailscale process output exceeded the {0}-byte safety limit")]
    OutputTooLarge(u64),
    #[error("tailscale command timed out")]
    Timeout,
    #[error("tailscale command failed with code {exit_code}: {stderr}")]
    CommandFailed { exit_code: i32, stderr: String },
    #[error("tailscale daemon is unavailable: {0}")]
    DaemonUnavailable(String),
}

pub fn parse_status(json: &str) -> Result<TailscaleStatus, TailscaleError> {
    let trimmed = json.trim().trim_start_matches('\u{feff}').trim();
    if let Ok(status) = serde_json::from_str(trimmed) {
        return Ok(status);
    }

    // Some Tailscale builds and wrapper launchers print a warning before or
    // after the JSON document. Keep status discovery compatible with those
    // builds without accepting an arbitrary truncated document: extract one
    // complete top-level object and still let serde validate its contents.
    let Some(start) = trimmed.find('{') else {
        return Err(TailscaleError::InvalidJson);
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err(TailscaleError::InvalidJson);
    };
    if start >= end {
        return Err(TailscaleError::InvalidJson);
    }

    serde_json::from_str(&trimmed[start..=end]).map_err(|_| TailscaleError::InvalidJson)
}

pub fn backend_plan(
    mode: TailscaleMode,
    state_dir: Option<&str>,
) -> Result<BackendPlan, TailscaleError> {
    match mode {
        TailscaleMode::Disabled => Ok(BackendPlan {
            mode,
            program: None,
            args: Vec::new(),
            requires_auth_key_injection: false,
        }),
        TailscaleMode::SystemDaemon => Ok(BackendPlan {
            mode,
            program: Some("tailscale".into()),
            args: vec!["status".into(), "--json".into()],
            requires_auth_key_injection: false,
        }),
        TailscaleMode::EmbeddedUserspace => {
            let state_dir = state_dir
                .filter(|path| !path.trim().is_empty())
                .ok_or(TailscaleError::MissingStateDirectory)?;
            Ok(BackendPlan {
                mode,
                program: Some("tailscaled".into()),
                args: vec![
                    "--tun=userspace-networking".into(),
                    format!("--state={state_dir}"),
                    "--socks5-server=127.0.0.1:0".into(),
                ],
                requires_auth_key_injection: true,
            })
        }
    }
}

impl TailscaleStatus {
    pub fn online_peers(&self) -> impl Iterator<Item = (&String, &TailscaleNode)> {
        self.peers
            .iter()
            .filter(|(_, node)| node.online.unwrap_or(false))
    }

    #[must_use]
    pub fn peer_addresses(&self) -> Vec<Address> {
        self.online_peers()
            .flat_map(|(_, node)| {
                node.tailscale_ips.iter().map(|ip| Address {
                    id: AddressId::new(),
                    kind: AddressKind::Tailscale,
                    hostname_or_ip: ip.clone(),
                    port: 22,
                    priority: 10,
                    enabled: true,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_reads_official_case_and_peer_map_shape() {
        let json = r#"{"Self":{"HostName":"kodework-windows","DNSName":"kodework-windows.tailnet.ts.net.","Online":true,"TailscaleIPs":["100.64.0.1"]},"Peer":{"node-key":{"HostName":"lab","DNSName":"lab.tailnet.ts.net.","Online":true,"TailscaleIPs":["100.64.0.2"],"FutureField":42}}}"#;
        let status = parse_status(json).unwrap_or_else(|_| unreachable!("fixture is valid JSON"));
        assert_eq!(
            status
                .self_node
                .as_ref()
                .and_then(|node| node.host_name.as_deref()),
            Some("kodework-windows")
        );
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peer_addresses().len(), 1);
    }

    #[test]
    fn offline_peers_are_not_candidates() {
        let json = r#"{"Peer":{"node-key":{"Online":false,"TailscaleIPs":["100.64.0.9"]}}}"#;
        let status = parse_status(json).unwrap_or_else(|_| unreachable!("fixture is valid JSON"));
        assert!(status.peer_addresses().is_empty());
    }

    #[test]
    fn backend_plan_never_contains_an_auth_key() {
        let system = backend_plan(TailscaleMode::SystemDaemon, None)
            .unwrap_or_else(|_| unreachable!("system plan"));
        assert_eq!(system.args, vec!["status", "--json"]);
        assert!(!system.requires_auth_key_injection);

        let embedded = backend_plan(
            TailscaleMode::EmbeddedUserspace,
            Some("C:/ProgramData/Kodework/tailscale.state"),
        )
        .unwrap_or_else(|_| unreachable!("embedded plan"));
        assert!(embedded.requires_auth_key_injection);
        assert!(embedded.args.iter().all(|arg| !arg.starts_with("tskey-")));
        assert_eq!(
            backend_plan(TailscaleMode::EmbeddedUserspace, None),
            Err(TailscaleError::MissingStateDirectory)
        );
    }

    #[test]
    fn invalid_json_is_typed() {
        assert_eq!(parse_status("not-json"), Err(TailscaleError::InvalidJson));
    }

    #[test]
    fn parser_accepts_bom_and_cli_warnings_around_json() {
        let output = "\u{feff}Warning: client and server versions differ\r\n{\"BackendState\":\"Running\",\"Peer\":{}}\r\nUpgrade recommended";
        let status =
            parse_status(output).unwrap_or_else(|_| unreachable!("embedded JSON is valid"));
        assert_eq!(status.backend_state.as_deref(), Some("Running"));
    }

    #[test]
    fn parser_does_not_accept_a_truncated_object() {
        assert_eq!(
            parse_status("warning {\"BackendState\":\"Running\""),
            Err(TailscaleError::InvalidJson)
        );
    }

    #[test]
    fn parser_accepts_official_needs_login_null_collections() {
        let json = r#"{
          "Version":"1.102.2-kodework.1",
          "BackendState":"NeedsLogin",
          "Self":{
            "HostName":"KING",
            "Online":false,
            "TailscaleIPs":null
          },
          "Peer":null
        }"#;
        let status = parse_status(json)
            .unwrap_or_else(|_| unreachable!("NeedsLogin status is an official response shape"));
        assert_eq!(status.backend_state.as_deref(), Some("NeedsLogin"));
        assert!(status.peers.is_empty());
        assert_eq!(
            status
                .self_node
                .as_ref()
                .map(|node| node.tailscale_ips.as_slice()),
            Some([].as_slice())
        );
    }
}
