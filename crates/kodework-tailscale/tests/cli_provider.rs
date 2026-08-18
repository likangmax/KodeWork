//! Tailscale CLI and provider tests with an injected fake runner.

use kodework_domain::{
    Address, AddressId, AddressKind, Host, HostId, RuntimeKind, TailscaleConfig, TailscaleMode,
};
use kodework_network::{AddressProvider, CandidateResolver, CandidateSource, ResolverPolicy};
use kodework_tailscale::cli::{TailscaleCli, TailscaleRunner};
use kodework_tailscale::provider::TailscaleAddressProvider;
use kodework_tailscale::TailscaleError;
use std::sync::Arc;
use std::time::Duration;

const ONLINE_PEERS: &str = r#"{
  "Self": {
    "HostName": "kodework-windows",
    "DNSName": "kodework-windows.tailnet.ts.net.",
    "Online": true,
    "TailscaleIPs": ["100.64.0.1"]
  },
  "Peer": {
    "node-a": {
      "HostName": "lab",
      "DNSName": "lab.tailnet.ts.net.",
      "Online": true,
      "TailscaleIPs": ["100.64.0.2"],
      "FutureField": 42
    },
    "node-b": {
      "HostName": "offline-box",
      "DNSName": "offline-box.tailnet.ts.net.",
      "Online": false,
      "TailscaleIPs": ["100.64.0.3"]
    }
  }
}"#;

/// Fake runner with a scripted outcome.
struct FakeRunner {
    outcome: Result<(i32, Vec<u8>, Vec<u8>), TailscaleError>,
}

#[async_trait::async_trait]
impl TailscaleRunner for FakeRunner {
    async fn run(
        &self,
        _args: &[&str],
        _timeout: Duration,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
        self.outcome.clone()
    }
}

fn cli_with(outcome: Result<(i32, Vec<u8>, Vec<u8>), TailscaleError>) -> TailscaleCli {
    TailscaleCli::new(Box::new(FakeRunner { outcome }), Duration::from_secs(2))
}

fn success(stdout: &str) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
    Ok((0, stdout.as_bytes().to_vec(), Vec::new()))
}

fn host_with_tailscale(device: Option<&str>, addresses: Vec<Address>) -> Host {
    Host {
        id: HostId::new(),
        label: "lab".into(),
        username: "alex".into(),
        port: 22,
        auth_ref: None,
        auth_mode: kodework_domain::AuthenticationMode::Password,
        private_key_path: None,
        default_remote_path: "/".into(),
        jump: None,
        addresses,
        tailscale: Some(TailscaleConfig {
            enabled: true,
            mode: TailscaleMode::SystemDaemon,
            device_name: device.map(String::from),
            auth_key_ref: None,
            state_dir: None,
        }),
        default_runtime: RuntimeKind::Tmux,
    }
}

fn ts_address(ip: &str) -> Address {
    Address {
        id: AddressId::new(),
        kind: AddressKind::Tailscale,
        hostname_or_ip: ip.into(),
        port: 22,
        priority: 10,
        enabled: true,
    }
}

#[tokio::test]
async fn status_parses_unknown_fields_and_reports_peers() {
    let cli = cli_with(success(ONLINE_PEERS));
    let status = cli
        .status()
        .await
        .unwrap_or_else(|error| unreachable!("status must parse: {error}"));
    assert_eq!(status.peers.len(), 2, "future fields must be ignored");
    assert_eq!(status.online_peers().count(), 1);
}

#[tokio::test]
async fn provider_keeps_online_and_drops_offline_addresses() {
    let provider = TailscaleAddressProvider::new(cli_with(success(ONLINE_PEERS)));
    let host = host_with_tailscale(
        None,
        vec![ts_address("100.64.0.2"), ts_address("100.64.0.3")],
    );
    let candidates = provider
        .candidates(&host)
        .await
        .unwrap_or_else(|error| unreachable!("provider: {error}"));
    assert_eq!(candidates.len(), 1, "only the online address survives");
    assert_eq!(candidates[0].address.hostname_or_ip, "100.64.0.2");
    assert_eq!(candidates[0].source, CandidateSource::Tailscale);
}

#[tokio::test]
async fn provider_discovers_device_peer_address() {
    let provider = TailscaleAddressProvider::new(cli_with(success(ONLINE_PEERS)));
    let host = host_with_tailscale(Some("lab"), vec![]);
    let candidates = provider
        .candidates(&host)
        .await
        .unwrap_or_else(|error| unreachable!("provider: {error}"));
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].address.hostname_or_ip, "100.64.0.2");
    assert_eq!(candidates[0].address.port, 22);
}

#[tokio::test]
async fn daemon_unavailable_is_typed_and_resolver_falls_back() {
    let provider = TailscaleAddressProvider::new(cli_with(Ok((
        1,
        Vec::new(),
        b"Failed to connect to local Tailscale daemon; run tailscaled to start it".to_vec(),
    ))));
    let host = host_with_tailscale(None, vec![ts_address("100.64.0.2")]);
    let error = provider
        .candidates(&host)
        .await
        .err()
        .unwrap_or_else(|| unreachable!("daemon failure must be an error"));
    assert!(format!("{error:?}").contains("tailscale"));

    // The resolver still resolves the manual fallback address.
    let mut manual_host = host.clone();
    manual_host.addresses.push(Address {
        id: AddressId::new(),
        kind: AddressKind::Manual,
        hostname_or_ip: "203.0.113.9".into(),
        port: 22,
        priority: 0,
        enabled: true,
    });
    let resolver = CandidateResolver::new(vec![Arc::new(provider)], ResolverPolicy::default());
    let diagnostics = resolver.provider_diagnostics(&manual_host).await;
    assert_eq!(diagnostics.len(), 1, "daemon issue must be surfaced");
    let candidates = resolver.candidates(&manual_host).await;
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.address.hostname_or_ip == "203.0.113.9"),
        "manual address survives provider failure, got: {candidates:?}"
    );
}

#[tokio::test]
async fn invalid_json_is_typed() {
    let cli = cli_with(success("not json at all"));
    assert_eq!(cli.status().await, Err(TailscaleError::InvalidJson));
}

#[tokio::test]
async fn non_zero_exit_without_daemon_keyword_is_command_failure() {
    let cli = cli_with(Ok((2, Vec::new(), b"flag needs an argument".to_vec())));
    let error = cli
        .status()
        .await
        .err()
        .unwrap_or_else(|| unreachable!("must fail"));
    assert!(matches!(error, TailscaleError::CommandFailed { .. }));
}

#[tokio::test]
async fn runner_timeout_is_bounded() {
    let cli = cli_with(Err(TailscaleError::Timeout));
    let error = cli
        .status()
        .await
        .err()
        .unwrap_or_else(|| unreachable!("must fail"));
    assert_eq!(error, TailscaleError::Timeout);
}

#[tokio::test]
async fn auth_registration_uses_cli_timeout_plus_cleanup_margin() {
    use std::sync::{Arc, Mutex};

    struct RecordingRunner(Arc<Mutex<Option<Duration>>>);

    #[async_trait::async_trait]
    impl TailscaleRunner for RecordingRunner {
        async fn run(
            &self,
            _args: &[&str],
            timeout: Duration,
        ) -> Result<(i32, Vec<u8>, Vec<u8>), TailscaleError> {
            *self.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(timeout);
            Ok((0, Vec::new(), Vec::new()))
        }
    }

    let observed = Arc::new(Mutex::new(None));
    let cli = TailscaleCli::new(
        Box::new(RecordingRunner(Arc::clone(&observed))),
        Duration::from_secs(10),
    );
    cli.up_with_auth_key_file(
        std::path::Path::new(r"C:\private\auth.tmp"),
        Duration::from_secs(20),
    )
    .await
    .unwrap_or_else(|error| unreachable!("recording runner succeeds: {error}"));
    assert_eq!(
        *observed.lock().unwrap_or_else(|error| error.into_inner()),
        Some(Duration::from_secs(25))
    );
}
#[tokio::test]
async fn device_match_does_not_fuzzy_match_similar_names() {
    const PEERS: &str = r#"{
      "Peer": {
        "node-a": {
          "HostName": "lab",
          "DNSName": "lab.tailnet.ts.net.",
          "Online": true,
          "TailscaleIPs": ["100.64.0.2"]
        },
        "node-b": {
          "HostName": "lab2",
          "DNSName": "lab2.tailnet.ts.net.",
          "Online": true,
          "TailscaleIPs": ["100.64.0.4"]
        }
      }
    }"#;
    let provider = TailscaleAddressProvider::new(cli_with(success(PEERS)));
    let host = host_with_tailscale(Some("lab"), vec![]);
    let candidates = provider
        .candidates(&host)
        .await
        .unwrap_or_else(|error| unreachable!("provider: {error}"));
    assert_eq!(candidates.len(), 1, "only the exact device matches");
    assert_eq!(candidates[0].address.hostname_or_ip, "100.64.0.2");
}
