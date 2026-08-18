#![forbid(unsafe_code)]

//! Tailscale discovery provider: verifies configured Tailscale addresses
//! against live `tailscale status` and discovers current peer IPs for a
//! configured device name (fallback-address switching).

use crate::cli::TailscaleCli;
use crate::runtime::TailscaleRuntime;
use crate::{TailscaleError, TailscaleStatus};
use kodework_domain::{Address, AddressId, AddressKind, Host};
use kodework_network::{AddressCandidate, AddressProvider, CandidateSource, NetworkError};
use std::sync::Arc;

/// AddressProvider backed by the tailscale CLI.
pub struct TailscaleAddressProvider {
    backend: ProviderBackend,
}

enum ProviderBackend {
    Cli(TailscaleCli),
    Runtime(Arc<TailscaleRuntime>),
}

impl TailscaleAddressProvider {
    #[must_use]
    pub fn new(cli: TailscaleCli) -> Self {
        Self {
            backend: ProviderBackend::Cli(cli),
        }
    }

    #[must_use]
    pub fn from_runtime(runtime: Arc<TailscaleRuntime>) -> Self {
        Self {
            backend: ProviderBackend::Runtime(runtime),
        }
    }
}

#[async_trait::async_trait]
impl AddressProvider for TailscaleAddressProvider {
    async fn candidates(&self, host: &Host) -> Result<Vec<AddressCandidate>, NetworkError> {
        let enabled = host.tailscale.as_ref().is_some_and(|config| config.enabled);
        let has_tailscale_address = host
            .addresses
            .iter()
            .any(|address| address.kind == AddressKind::Tailscale && address.enabled);
        if !enabled && !has_tailscale_address {
            return Ok(Vec::new());
        }

        let status = match match &self.backend {
            ProviderBackend::Cli(cli) => cli.status().await,
            ProviderBackend::Runtime(runtime) => {
                runtime.status_for_config(host.tailscale.as_ref()).await
            }
        } {
            Ok(status) => status,
            Err(TailscaleError::DaemonUnavailable(message)) => {
                return Err(NetworkError::ProviderUnavailable("tailscale", message));
            }
            // A missing executable is an availability problem too; surface it
            // distinctly so the UI can offer install guidance.
            Err(TailscaleError::Spawn(message)) => {
                return Err(NetworkError::ProviderUnavailable("tailscale", message));
            }
            Err(error) => return Err(NetworkError::Provider(error.to_string())),
        };

        let mut candidates = discover_candidates(host, &status);
        if let ProviderBackend::Runtime(runtime) = &self.backend {
            for candidate in &mut candidates {
                if candidate.address.kind == AddressKind::Tailscale {
                    candidate.proxy = runtime
                        .proxy_spec(
                            host.tailscale.as_ref(),
                            &candidate.address.hostname_or_ip,
                            candidate.address.port,
                        )
                        .await;
                }
            }
        }
        Ok(candidates)
    }
}

/// Pure candidate discovery: configured Tailscale addresses that are
/// online, plus device-name matched peers (fallback addresses).
#[must_use]
pub fn discover_candidates(host: &Host, status: &TailscaleStatus) -> Vec<AddressCandidate> {
    let mut out = Vec::new();

    // 1) Configured Tailscale addresses: keep only online ones.
    for address in host
        .addresses
        .iter()
        .filter(|address| address.kind == AddressKind::Tailscale && address.enabled)
    {
        if is_online(status, &address.hostname_or_ip) {
            out.push(AddressCandidate {
                address: address.clone(),
                source: CandidateSource::Tailscale,
                proxy: None,
            });
        }
    }

    // 2) Device-name matched peers become candidate addresses.
    if let Some(device) = host
        .tailscale
        .as_ref()
        .and_then(|config| config.device_name.as_ref())
    {
        for (_, peer) in status.online_peers() {
            let name_matches = peer.host_name.as_deref().is_some_and(|name| name == device)
                || peer.dns_name.as_deref().is_some_and(|dns| {
                    // Exact device prefix: "lab" matches "lab.tailnet...",
                    // never "lab2.tailnet...".
                    dns == device
                        || dns
                            .strip_prefix(device)
                            .is_some_and(|rest| rest.starts_with('.'))
                });
            if name_matches {
                for ip in &peer.tailscale_ips {
                    out.push(AddressCandidate {
                        address: Address {
                            id: AddressId::new(),
                            kind: AddressKind::Tailscale,
                            hostname_or_ip: ip.clone(),
                            port: host.port,
                            priority: 10,
                            enabled: true,
                        },
                        source: CandidateSource::Tailscale,
                        proxy: None,
                    });
                }
            }
        }
    }

    out
}

fn is_online(status: &TailscaleStatus, hostname_or_ip: &str) -> bool {
    if let Some(node) = status.peers.get(hostname_or_ip) {
        return node.online.unwrap_or(false);
    }
    if let Some(node) = status.self_node.as_ref() {
        if node.tailscale_ips.iter().any(|ip| ip == hostname_or_ip) {
            return node.online.unwrap_or(false);
        }
    }
    status.peers.values().any(|node| {
        node.online.unwrap_or(false) && node.tailscale_ips.iter().any(|ip| ip == hostname_or_ip)
    })
}
