#![forbid(unsafe_code)]

//! Address candidate resolution: configured addresses plus async discovery
//! providers (Tailscale, future LAN scans), merged and ordered by policy.

use kodework_domain::{Address, AddressKind, Host};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressCandidate {
    pub address: Address,
    pub source: CandidateSource,
    /// Optional process-backed transport (for example `tailscale nc`) used
    /// instead of a direct TCP socket for userspace networking.
    pub proxy: Option<ProxySpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateSource {
    Configured,
    Tailscale,
    Discovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    Timeout,
    Unreachable,
    TailscaleUnavailable,
    AuthenticationFailed,
    HostKeyChanged,
    InvalidConfiguration,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NetworkError {
    #[error("no enabled address candidates")]
    NoCandidates,
    #[error("discovery provider failed: {0}")]
    Provider(String),
    #[error("provider {0} is not available: {1}")]
    ProviderUnavailable(&'static str, String),
}

/// Discovery provider producing additional candidates for a host.
#[async_trait::async_trait]
pub trait AddressProvider: Send + Sync {
    /// Returns extra candidates (e.g. Tailscale peers) or a typed error
    /// when the provider is unavailable (e.g. daemon not running).
    async fn candidates(&self, host: &Host) -> Result<Vec<AddressCandidate>, NetworkError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ResolverPolicy {
    pub prefer_tailscale: bool,
    pub allow_public_fallback: bool,
}

impl Default for ResolverPolicy {
    fn default() -> Self {
        Self {
            prefer_tailscale: true,
            allow_public_fallback: true,
        }
    }
}

/// Combines configured addresses with discovery providers, deduplicates
/// and orders candidates by policy. Provider failures degrade gracefully:
/// configured candidates still resolve, and the failure is surfaced so the
/// UI can explain why Tailscale is missing.
#[derive(Clone)]
pub struct CandidateResolver {
    providers: Vec<Arc<dyn AddressProvider>>,
    policy: ResolverPolicy,
}

impl CandidateResolver {
    #[must_use]
    pub fn new(providers: Vec<Arc<dyn AddressProvider>>, policy: ResolverPolicy) -> Self {
        Self { providers, policy }
    }

    /// Merged, deduplicated, ordered candidates.
    pub async fn candidates(&self, host: &Host) -> Vec<AddressCandidate> {
        let mut merged: Vec<AddressCandidate> =
            configured_candidates(host, self.policy).unwrap_or_default();
        for provider in &self.providers {
            if let Ok(candidates) = provider.candidates(host).await {
                merged.extend(candidates);
            }
        }
        deduplicate(merged, self.policy)
    }

    /// Returns provider diagnostics (e.g. Tailscale daemon unavailable) so
    /// callers can show the user why discovery is missing.
    pub async fn provider_diagnostics(&self, host: &Host) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for provider in &self.providers {
            match provider.candidates(host).await {
                Ok(_) => {}
                Err(NetworkError::ProviderUnavailable(name, message)) => {
                    out.push((name.to_string(), message));
                }
                Err(NetworkError::Provider(message)) => {
                    out.push(("provider".to_string(), message));
                }
                Err(NetworkError::NoCandidates) => {}
            }
        }
        out
    }
}

pub fn configured_candidates(
    host: &Host,
    policy: ResolverPolicy,
) -> Result<Vec<AddressCandidate>, NetworkError> {
    let mut candidates: Vec<_> = host
        .addresses
        .iter()
        .filter(|address| address.enabled)
        .filter(|address| policy.allow_public_fallback || address.kind != AddressKind::Public)
        .cloned()
        .map(|address| AddressCandidate {
            source: CandidateSource::Configured,
            address,
            proxy: None,
        })
        .collect();
    if candidates.is_empty() {
        return Err(NetworkError::NoCandidates);
    }
    candidates.sort_by_key(|candidate| sort_key(candidate, policy));
    Ok(candidates)
}

#[must_use]
pub fn should_try_next(class: FailureClass) -> bool {
    matches!(
        class,
        FailureClass::Timeout | FailureClass::Unreachable | FailureClass::TailscaleUnavailable
    )
}

fn score(kind: AddressKind, policy: ResolverPolicy) -> i32 {
    match kind {
        AddressKind::Manual => 0,
        AddressKind::Tailscale if policy.prefer_tailscale => 1,
        AddressKind::Lan => 2,
        AddressKind::Tailscale => 3,
        AddressKind::Public if policy.allow_public_fallback => 4,
        AddressKind::JumpHost => 5,
        AddressKind::Public => 99,
    }
}

fn deduplicate(candidates: Vec<AddressCandidate>, policy: ResolverPolicy) -> Vec<AddressCandidate> {
    let mut seen: HashSet<(u16, String)> = HashSet::new();
    let mut out = Vec::new();
    for candidate in candidates {
        let key = (
            candidate.address.port,
            candidate.address.hostname_or_ip.clone(),
        );
        if seen.insert(key.clone()) {
            out.push(candidate);
        } else if let Some(existing) = out
            .iter_mut()
            .find(|item| item.address.port == key.0 && item.address.hostname_or_ip == key.1)
        {
            // A discovered userspace candidate may carry a transport proxy
            // while the same address was already configured manually.
            if existing.proxy.is_none() && candidate.proxy.is_some() {
                *existing = candidate;
            }
        }
    }
    out.sort_by_key(|candidate| sort_key(candidate, policy));
    out
}

fn sort_key(candidate: &AddressCandidate, policy: ResolverPolicy) -> (i32, i32) {
    // Lower policy score wins; higher user priority wins within a class.
    (
        score(candidate.address.kind, policy),
        -candidate.address.priority,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{AddressId, HostId, RuntimeKind};

    fn host_with(addresses: Vec<Address>) -> Host {
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
            tailscale: None,
            default_runtime: RuntimeKind::Tmux,
        }
    }

    fn address(kind: AddressKind, hostname: &str) -> Address {
        Address {
            id: AddressId::new(),
            kind,
            hostname_or_ip: hostname.into(),
            port: 22,
            priority: 0,
            enabled: true,
        }
    }

    struct StaticProvider {
        candidates: Vec<AddressCandidate>,
        error: Option<NetworkError>,
    }

    #[async_trait::async_trait]
    impl AddressProvider for StaticProvider {
        async fn candidates(&self, _host: &Host) -> Result<Vec<AddressCandidate>, NetworkError> {
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            Ok(self.candidates.clone())
        }
    }

    #[tokio::test]
    async fn resolver_merges_and_deduplicates() {
        let host = host_with(vec![
            address(AddressKind::Lan, "192.168.1.2"),
            address(AddressKind::Tailscale, "100.64.0.2"),
        ]);
        let provider = StaticProvider {
            candidates: vec![AddressCandidate {
                address: address(AddressKind::Tailscale, "100.64.0.2"),
                source: CandidateSource::Tailscale,
                proxy: None,
            }],
            error: None,
        };
        let resolver = CandidateResolver::new(vec![Arc::new(provider)], ResolverPolicy::default());
        let candidates = resolver.candidates(&host).await;
        assert_eq!(candidates.len(), 2, "duplicate 100.64.0.2 must collapse");
        assert_eq!(candidates[0].address.hostname_or_ip, "100.64.0.2");
        assert_eq!(candidates[1].address.hostname_or_ip, "192.168.1.2");
    }

    #[tokio::test]
    async fn resolver_surfaces_provider_failures() {
        let host = host_with(vec![address(AddressKind::Manual, "203.0.113.5")]);
        let provider = StaticProvider {
            candidates: Vec::new(),
            error: Some(NetworkError::ProviderUnavailable(
                "tailscale",
                "daemon not running".into(),
            )),
        };
        let resolver = CandidateResolver::new(vec![Arc::new(provider)], ResolverPolicy::default());
        let diagnostics = resolver.provider_diagnostics(&host).await;
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].0, "tailscale");
        // Manual address still resolves even though the provider failed.
        let candidates = resolver.candidates(&host).await;
        assert_eq!(candidates.len(), 1);
    }

    #[tokio::test]
    async fn resolver_keeps_discovered_candidates() {
        let host = host_with(Vec::new());
        let discovered = address(AddressKind::Tailscale, "100.64.0.9");
        let provider = StaticProvider {
            candidates: vec![AddressCandidate {
                address: discovered,
                source: CandidateSource::Discovered,
                proxy: None,
            }],
            error: None,
        };
        let resolver = CandidateResolver::new(vec![Arc::new(provider)], ResolverPolicy::default());
        let candidates = resolver.candidates(&host).await;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CandidateSource::Discovered);
    }
}
