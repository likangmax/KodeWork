#![forbid(unsafe_code)]

//! Bridges the SQLite host-key repository into the SSH host-key broker.

use kodework_domain::HostId;
use kodework_ssh::host_key::{HostKeyInfo, KnownHosts};
use kodework_storage::host_keys::{HostKeyIdentityRecord, HostKeyRecord, HostKeyRepository};
use std::sync::{Arc, Mutex};

/// Persists host keys in the app database (fingerprints are public
/// metadata; no secret material is stored).
pub struct SqliteKnownHosts {
    database: Arc<Mutex<kodework_storage::Database>>,
}

impl SqliteKnownHosts {
    #[must_use]
    pub fn new(database: Arc<Mutex<kodework_storage::Database>>) -> Self {
        Self { database }
    }
}

impl KnownHosts for SqliteKnownHosts {
    fn lookup(&self, hostname: &str, port: u16) -> Option<HostKeyInfo> {
        let guard = self.database.lock().ok()?;
        let repo = HostKeyRepository::new(guard.connection());
        repo.get(hostname, port).ok().flatten().map(record_to_info)
    }

    fn save(&self, hostname: &str, port: u16, key: &HostKeyInfo) -> Result<(), String> {
        let guard = self
            .database
            .lock()
            .map_err(|_| "host-key database lock poisoned".to_string())?;
        let repo = HostKeyRepository::new(guard.connection());
        let record = HostKeyRecord::new(
            hostname.to_string(),
            port,
            key.algorithm.clone(),
            key.fingerprint.clone(),
            key.key_blob_base64.clone(),
        );
        repo.save(&record).map_err(|error| error.to_string())
    }

    fn lookup_for_host(&self, host_id: HostId, hostname: &str, port: u16) -> Option<HostKeyInfo> {
        let guard = self.database.lock().ok()?;
        let repo = HostKeyRepository::new(guard.connection());
        if let Some(record) = repo.get_for_host(host_id).ok().flatten() {
            return Some(HostKeyInfo {
                hostname: hostname.to_string(),
                port,
                algorithm: record.algorithm,
                fingerprint: record.fingerprint,
                key_blob_base64: record.key_blob_base64,
            });
        }
        let legacy = repo.get(hostname, port).ok().flatten()?;
        let info = record_to_info(legacy);
        let promoted = HostKeyIdentityRecord::new(
            host_id,
            info.algorithm.clone(),
            info.fingerprint.clone(),
            info.key_blob_base64.clone(),
        );
        // A matching legacy address record is authoritative enough to avoid a
        // new prompt. Promote it immediately so future fallback addresses for
        // this logical Host use the same identity.
        repo.save_for_host(&promoted).ok()?;
        Some(HostKeyInfo {
            hostname: hostname.to_string(),
            port,
            ..info
        })
    }

    fn save_for_host(
        &self,
        host_id: HostId,
        _hostname: &str,
        _port: u16,
        key: &HostKeyInfo,
    ) -> Result<(), String> {
        let guard = self
            .database
            .lock()
            .map_err(|_| "host-key database lock poisoned".to_string())?;
        let repo = HostKeyRepository::new(guard.connection());
        let record = HostKeyIdentityRecord::new(
            host_id,
            key.algorithm.clone(),
            key.fingerprint.clone(),
            key.key_blob_base64.clone(),
        );
        repo.save_for_host(&record)
            .map_err(|error| error.to_string())
    }
}

fn record_to_info(record: HostKeyRecord) -> HostKeyInfo {
    HostKeyInfo {
        hostname: record.hostname,
        port: record.port,
        algorithm: record.algorithm,
        fingerprint: record.fingerprint,
        key_blob_base64: record.key_blob_base64,
    }
}
