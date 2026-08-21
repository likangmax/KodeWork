#![forbid(unsafe_code)]

//! Host-key verification policy and decision broker.
//!
//! Policy rules (GOAL §7.4): a known fingerprint passes automatically; a
//! changed fingerprint is a hard failure; an unknown fingerprint must be
//! decided by the user (trust once / trust and save / reject) with a
//! deadline. Unconditional acceptance is never allowed.

use crate::SshError;
use kodework_domain::HostId;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::PublicKeyBase64;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

/// Default deadline for a user host-key decision.
pub const DEFAULT_DECISION_TIMEOUT: Duration = Duration::from_secs(60);

/// Information about a server public key, safe to show to the user and to
/// persist. Never contains secret material.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostKeyInfo {
    pub hostname: String,
    pub port: u16,
    /// Key algorithm name, e.g. `ssh-ed25519`.
    pub algorithm: String,
    /// OpenSSH-style fingerprint, e.g. `SHA256:AbC...`.
    pub fingerprint: String,
    /// Standard base64 key blob used for persistent comparison.
    pub key_blob_base64: String,
}

/// User decision for an unknown host key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Accept for this connection only; do not persist.
    TrustOnce,
    /// Accept and persist so future connections pass automatically.
    TrustAndSave,
    /// Reject the connection.
    Reject,
}

/// Persistent store for trusted host keys (metadata only).
pub trait KnownHosts: Send + Sync {
    fn lookup(&self, hostname: &str, port: u16) -> Result<Option<HostKeyInfo>, String>;
    fn save(&self, hostname: &str, port: u16, key: &HostKeyInfo) -> Result<(), String>;

    fn lookup_for_host(
        &self,
        _host_id: HostId,
        hostname: &str,
        port: u16,
    ) -> Result<Option<HostKeyInfo>, String> {
        self.lookup(hostname, port)
    }

    fn save_for_host(
        &self,
        _host_id: HostId,
        hostname: &str,
        port: u16,
        key: &HostKeyInfo,
    ) -> Result<(), String> {
        self.save(hostname, port, key)
    }
}

/// In-memory known-hosts store (tests and pre-SQLite use).
#[derive(Default)]
pub struct MemoryKnownHosts {
    keys: Mutex<HashMap<(String, u16), HostKeyInfo>>,
    host_keys: Mutex<HashMap<HostId, HostKeyInfo>>,
}

impl MemoryKnownHosts {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl KnownHosts for MemoryKnownHosts {
    fn lookup(&self, hostname: &str, port: u16) -> Result<Option<HostKeyInfo>, String> {
        Ok(self
            .keys
            .lock()
            .map_err(|_| "known-hosts lock poisoned".to_string())?
            .get(&(hostname.to_string(), port))
            .cloned())
    }

    fn save(&self, hostname: &str, port: u16, key: &HostKeyInfo) -> Result<(), String> {
        let mut guard = self
            .keys
            .lock()
            .map_err(|_| "known-hosts lock poisoned".to_string())?;
        guard.insert((hostname.to_string(), port), key.clone());
        Ok(())
    }

    fn lookup_for_host(
        &self,
        host_id: HostId,
        hostname: &str,
        port: u16,
    ) -> Result<Option<HostKeyInfo>, String> {
        let host_key = self
            .host_keys
            .lock()
            .map_err(|_| "known-hosts lock poisoned".to_string())?
            .get(&host_id)
            .cloned();
        match host_key {
            Some(key) => Ok(Some(key)),
            None => self.lookup(hostname, port),
        }
    }

    fn save_for_host(
        &self,
        host_id: HostId,
        _hostname: &str,
        _port: u16,
        key: &HostKeyInfo,
    ) -> Result<(), String> {
        self.host_keys
            .lock()
            .map_err(|_| "known-hosts lock poisoned".to_string())?
            .insert(host_id, key.clone());
        Ok(())
    }
}

/// A pending host-key decision request that the UI/core layer can answer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostKeyRequest {
    pub request_id: u64,
    pub info: HostKeyInfo,
}

/// Routes `check_server_key` calls through the configured policy store and
/// surfaces unknown-key decisions to the caller. The verifier is async, so
/// no thread is blocked while a decision is pending.
pub struct HostKeyBroker {
    known: Arc<dyn KnownHosts>,
    requests: Mutex<HashMap<u64, oneshot::Sender<HostKeyDecision>>>,
    pending: Mutex<VecDeque<HostKeyRequest>>,
    next_id: AtomicU64,
    decision_timeout: Duration,
}

impl std::fmt::Debug for HostKeyBroker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pending = self.pending.lock().map(|guard| guard.len()).unwrap_or(0);
        formatter
            .debug_struct("HostKeyBroker")
            .field("pending_requests", &pending)
            .field("decision_timeout", &self.decision_timeout)
            .finish()
    }
}

impl HostKeyBroker {
    #[must_use]
    pub fn new(known: Arc<dyn KnownHosts>, decision_timeout: Duration) -> Self {
        Self {
            known,
            requests: Mutex::new(HashMap::new()),
            pending: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            decision_timeout,
        }
    }

    /// Build a fingerprint for display and comparison.
    #[must_use]
    pub fn fingerprint_for(key: &PublicKey) -> String {
        key.fingerprint(HashAlg::Sha256).to_string()
    }

    /// Take all pending requests. Used by the UI layer to render dialogs.
    pub fn drain_requests(&self) -> Vec<HostKeyRequest> {
        let mut guard = match self.pending.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        guard.drain(..).collect()
    }

    /// Answer a pending request. Returns false when the request already
    /// timed out or was answered.
    pub fn answer(&self, request_id: u64, decision: HostKeyDecision) -> bool {
        let sender = match self.requests.lock() {
            Ok(mut guard) => guard.remove(&request_id),
            Err(_) => return false,
        };
        match sender {
            Some(sender) => sender.send(decision).is_ok(),
            None => false,
        }
    }

    /// Verify the server key for `hostname:port`.
    ///
    /// - known match → `Ok(true)`
    /// - known mismatch → `Err(HostKeyChanged)` (hard failure)
    /// - unknown → pending request; outcome by decision; timeout → `Err`
    pub async fn verify(
        &self,
        hostname: &str,
        port: u16,
        key: &PublicKey,
    ) -> Result<bool, SshError> {
        self.verify_for_host(None, hostname, port, key).await
    }

    pub async fn verify_for_host(
        &self,
        host_id: Option<HostId>,
        hostname: &str,
        port: u16,
        key: &PublicKey,
    ) -> Result<bool, SshError> {
        let fingerprint = Self::fingerprint_for(key);
        let info = HostKeyInfo {
            hostname: hostname.to_string(),
            port,
            algorithm: key.algorithm().to_string(),
            fingerprint: fingerprint.clone(),
            key_blob_base64: key.public_key_base64(),
        };

        let saved = match host_id {
            Some(id) => self
                .known
                .lookup_for_host(id, hostname, port)
                .map_err(SshError::HostKeyStoreUnavailable)?,
            None => self
                .known
                .lookup(hostname, port)
                .map_err(SshError::HostKeyStoreUnavailable)?,
        };
        if let Some(saved) = saved {
            if saved.fingerprint == fingerprint && !saved.key_blob_base64.is_empty() {
                if saved.key_blob_base64 == info.key_blob_base64 {
                    return Ok(true);
                }
            } else if saved.fingerprint == fingerprint {
                return Ok(true);
            }
            return Err(SshError::HostKeyChanged);
        }

        let (sender, receiver) = oneshot::channel();
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut guard = self.requests.lock().map_err(|_| {
                SshError::InvalidConfiguration("host-key broker lock poisoned".into())
            })?;
            guard.insert(request_id, sender);
        }
        {
            let mut guard = self.pending.lock().map_err(|_| {
                SshError::InvalidConfiguration("host-key broker lock poisoned".into())
            })?;
            guard.push_back(HostKeyRequest {
                request_id,
                info: info.clone(),
            });
        }

        let outcome = tokio::time::timeout(self.decision_timeout, receiver).await;
        let result = match outcome {
            Ok(Ok(HostKeyDecision::TrustOnce)) => Ok(true),
            Ok(Ok(HostKeyDecision::TrustAndSave)) => {
                match host_id {
                    Some(host_id) => self.known.save_for_host(host_id, hostname, port, &info),
                    None => self.known.save(hostname, port, &info),
                }
                .map_err(SshError::InvalidConfiguration)?;
                Ok(true)
            }
            Ok(Ok(HostKeyDecision::Reject)) | Ok(Err(_)) => Err(SshError::HostKeyRejected),
            Err(_) => Err(SshError::HostKeyDecisionTimeout),
        };
        // The decision is no longer awaited; drop the pending entry so the
        // UI never sees a stale request and the map does not leak senders.
        self.forget(request_id);
        result
    }

    fn forget(&self, request_id: u64) {
        if let Ok(mut guard) = self.requests.lock() {
            guard.remove(&request_id);
        }
        if let Ok(mut guard) = self.pending.lock() {
            guard.retain(|request| request.request_id != request_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::ssh_key::PrivateKey;

    struct BrokenKnownHosts;

    impl KnownHosts for BrokenKnownHosts {
        fn lookup(&self, _hostname: &str, _port: u16) -> Result<Option<HostKeyInfo>, String> {
            Err("database unavailable".to_string())
        }

        fn save(&self, _hostname: &str, _port: u16, _key: &HostKeyInfo) -> Result<(), String> {
            Ok(())
        }
    }

    fn test_key() -> PublicKey {
        let mut rng = rand::rng();
        let private = PrivateKey::random(&mut rng, russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap_or_else(|error| unreachable!("test key generation failed: {error}"));
        private.public_key().clone()
    }

    #[tokio::test]
    async fn unknown_key_waits_for_decision() {
        let broker = Arc::new(HostKeyBroker::new(
            Arc::new(MemoryKnownHosts::new()),
            Duration::from_secs(5),
        ));
        let key = test_key();
        let broker_task = Arc::clone(&broker);
        let handle = tokio::spawn(async move { broker_task.verify("lab.example", 22, &key).await });
        // wait until a request is pending (drain consumes the queue, so
        // keep the last drained batch)
        let mut requests = Vec::new();
        for _ in 0..50 {
            requests = broker.drain_requests();
            if !requests.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(requests.len(), 1, "one pending request expected");
        let request_id = requests[0].request_id;
        assert!(request_id > 0);
        assert!(broker.answer(request_id, HostKeyDecision::TrustOnce));
        let result = handle
            .await
            .unwrap_or_else(|error| unreachable!("verify task join failed: {error}"));
        assert_eq!(result, Ok(true));
    }

    #[tokio::test]
    async fn changed_key_is_hard_failure() {
        let known = Arc::new(MemoryKnownHosts::new());
        let first = test_key();
        known
            .save(
                "lab.example",
                22,
                &HostKeyInfo {
                    hostname: "lab.example".into(),
                    port: 22,
                    algorithm: first.algorithm().to_string(),
                    fingerprint: HostKeyBroker::fingerprint_for(&first),
                    key_blob_base64: first.public_key_base64(),
                },
            )
            .unwrap_or_else(|error| unreachable!("test save failed: {error}"));
        let broker = HostKeyBroker::new(known, Duration::from_secs(5));
        let second = test_key();
        let result = broker.verify("lab.example", 22, &second).await;
        assert_eq!(result, Err(SshError::HostKeyChanged));
    }

    #[tokio::test]
    async fn matching_key_passes_without_prompt() {
        let known = Arc::new(MemoryKnownHosts::new());
        let key = test_key();
        known
            .save(
                "lab.example",
                22,
                &HostKeyInfo {
                    hostname: "lab.example".into(),
                    port: 22,
                    algorithm: key.algorithm().to_string(),
                    fingerprint: HostKeyBroker::fingerprint_for(&key),
                    key_blob_base64: key.public_key_base64(),
                },
            )
            .unwrap_or_else(|error| unreachable!("test save failed: {error}"));
        let broker = HostKeyBroker::new(known, Duration::from_secs(5));
        let result = broker.verify("lab.example", 22, &key).await;
        assert_eq!(result, Ok(true));
        assert!(
            broker.drain_requests().is_empty(),
            "no prompt for known key"
        );
    }

    #[tokio::test]
    async fn known_hosts_store_error_blocks_without_prompt() {
        let broker = HostKeyBroker::new(Arc::new(BrokenKnownHosts), Duration::from_secs(5));
        let result = broker.verify("lab.example", 22, &test_key()).await;
        assert!(matches!(
            result,
            Err(SshError::HostKeyStoreUnavailable(message)) if message == "database unavailable"
        ));
        assert!(
            broker.drain_requests().is_empty(),
            "storage failures must not turn into an unknown-key prompt"
        );
    }

    #[tokio::test]
    async fn logical_host_identity_is_shared_across_addresses() {
        let known = Arc::new(MemoryKnownHosts::new());
        let host_id = HostId::new();
        let key = test_key();
        let info = HostKeyInfo {
            hostname: "100.64.0.10".into(),
            port: 22,
            algorithm: key.algorithm().to_string(),
            fingerprint: HostKeyBroker::fingerprint_for(&key),
            key_blob_base64: key.public_key_base64(),
        };
        known
            .save_for_host(host_id, "100.64.0.10", 22, &info)
            .unwrap_or_else(|error| unreachable!("test save failed: {error}"));
        let broker = HostKeyBroker::new(known, Duration::from_secs(5));

        assert_eq!(
            broker
                .verify_for_host(Some(host_id), "192.168.1.10", 22, &key)
                .await,
            Ok(true)
        );
        assert_eq!(
            broker
                .verify_for_host(Some(host_id), "203.0.113.10", 22, &test_key())
                .await,
            Err(SshError::HostKeyChanged)
        );
        assert!(broker.drain_requests().is_empty());
    }

    #[tokio::test]
    async fn timed_out_decision_is_forgotten() {
        // A decision that times out must leave no pending request and no
        // lingering sender behind (leak guard for long-lived brokers).
        let broker = Arc::new(HostKeyBroker::new(
            Arc::new(MemoryKnownHosts::new()),
            Duration::from_millis(60),
        ));
        let key = test_key();
        let start = std::time::Instant::now();
        let result = broker.verify("timeout.example", 22, &key).await;
        assert!(
            start.elapsed() >= Duration::from_millis(50),
            "verify must respect the decision timeout"
        );
        assert_eq!(result, Err(SshError::HostKeyDecisionTimeout));
        assert!(
            broker.drain_requests().is_empty(),
            "timed-out decision must not leave a stale pending request"
        );
    }
}
