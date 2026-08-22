#![forbid(unsafe_code)]

//! Host-key persistence (metadata only; keys are not secrets).

use crate::{now_millis, StorageError};
use kodework_domain::HostId;
use rusqlite::{params, Connection, OptionalExtension};

/// Stored host-key record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyRecord {
    pub hostname: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    pub key_blob_base64: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Host-key identity bound to one logical workstation rather than one
/// address. This lets LAN, Tailscale, and public fallback paths share the
/// same trust decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyIdentityRecord {
    pub host_id: HostId,
    pub algorithm: String,
    pub fingerprint: String,
    pub key_blob_base64: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl HostKeyIdentityRecord {
    #[must_use]
    pub fn new(
        host_id: HostId,
        algorithm: String,
        fingerprint: String,
        key_blob_base64: String,
    ) -> Self {
        let now = now_millis();
        Self {
            host_id,
            algorithm,
            fingerprint,
            key_blob_base64,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }
}

impl HostKeyRecord {
    #[must_use]
    pub fn new(
        hostname: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        key_blob_base64: String,
    ) -> Self {
        let now = now_millis();
        Self {
            hostname,
            port,
            algorithm,
            fingerprint,
            key_blob_base64,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }
}

pub struct HostKeyRepository<'a> {
    connection: &'a Connection,
}

impl<'a> HostKeyRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn get(&self, hostname: &str, port: u16) -> Result<Option<HostKeyRecord>, StorageError> {
        let record = self
            .connection
            .query_row(
                "SELECT hostname, port, algorithm, fingerprint, key_blob_base64, created_at_ms, updated_at_ms
                 FROM host_keys WHERE hostname = ?1 AND port = ?2",
                params![hostname, port],
                |row| {
                    Ok(HostKeyRecord {
                        hostname: row.get(0)?,
                        port: row.get(1)?,
                        algorithm: row.get(2)?,
                        fingerprint: row.get(3)?,
                        key_blob_base64: row.get(4)?,
                        created_at_ms: row.get(5)?,
                        updated_at_ms: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    pub fn save(&self, record: &HostKeyRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO host_keys (hostname, port, algorithm, fingerprint, key_blob_base64, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(hostname, port) DO UPDATE SET
                algorithm = excluded.algorithm,
                fingerprint = excluded.fingerprint,
                key_blob_base64 = excluded.key_blob_base64,
                updated_at_ms = excluded.updated_at_ms",
            params![
                record.hostname,
                record.port,
                record.algorithm,
                record.fingerprint,
                record.key_blob_base64,
                record.created_at_ms,
                record.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, hostname: &str, port: u16) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM host_keys WHERE hostname = ?1 AND port = ?2",
            params![hostname, port],
        )?;
        Ok(changed != 0)
    }

    pub fn list(&self) -> Result<Vec<HostKeyRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT hostname, port, algorithm, fingerprint, key_blob_base64, created_at_ms, updated_at_ms
             FROM host_keys ORDER BY hostname, port",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(HostKeyRecord {
                hostname: row.get(0)?,
                port: row.get(1)?,
                algorithm: row.get(2)?,
                fingerprint: row.get(3)?,
                key_blob_base64: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_for_host(
        &self,
        host_id: HostId,
        algorithm: &str,
    ) -> Result<Option<HostKeyIdentityRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT host_id, algorithm, fingerprint, key_blob_base64, created_at_ms, updated_at_ms
                 FROM host_key_identities WHERE host_id = ?1 AND algorithm = ?2",
                params![host_id.as_uuid().as_bytes().to_vec(), algorithm],
                |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    let uuid = uuid::Uuid::from_slice(&bytes).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Blob,
                            Box::new(error),
                        )
                    })?;
                    Ok(HostKeyIdentityRecord {
                        host_id: HostId::from_uuid(uuid),
                        algorithm: row.get(1)?,
                        fingerprint: row.get(2)?,
                        key_blob_base64: row.get(3)?,
                        created_at_ms: row.get(4)?,
                        updated_at_ms: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn save_for_host(&self, record: &HostKeyIdentityRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO host_key_identities (host_id, algorithm, fingerprint, key_blob_base64, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(host_id, algorithm) DO UPDATE SET
                fingerprint = excluded.fingerprint,
                key_blob_base64 = excluded.key_blob_base64,
                updated_at_ms = excluded.updated_at_ms",
            params![
                record.host_id.as_uuid().as_bytes().to_vec(),
                record.algorithm,
                record.fingerprint,
                record.key_blob_base64,
                record.created_at_ms,
                record.updated_at_ms,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    #[test]
    fn host_key_round_trip() {
        let database = Database::open_in_memory()
            .unwrap_or_else(|error| unreachable!("in-memory database: {error}"));
        let repo = HostKeyRepository::new(database.connection());
        let record = HostKeyRecord::new(
            "lab.example".into(),
            22,
            "ssh-ed25519".into(),
            "SHA256:test-fingerprint".into(),
            "dGVzdC1ibG9i".into(),
        );
        assert!(repo.save(&record).is_ok());
        let loaded = repo
            .get("lab.example", 22)
            .unwrap_or_else(|error| unreachable!("get: {error}"))
            .unwrap_or_else(|| unreachable!("record must exist"));
        assert_eq!(loaded, record);
        assert_eq!(repo.get("other", 22).ok().flatten(), None);
        assert_eq!(repo.list().map(|keys| keys.len()).ok(), Some(1));
        assert!(repo.delete("lab.example", 22).ok() == Some(true));
        assert_eq!(repo.get("lab.example", 22).ok().flatten(), None);
    }

    #[test]
    fn host_key_upsert_updates_fingerprint() {
        let database = Database::open_in_memory()
            .unwrap_or_else(|error| unreachable!("in-memory database: {error}"));
        let repo = HostKeyRepository::new(database.connection());
        let first = HostKeyRecord::new(
            "lab.example".into(),
            22,
            "ssh-ed25519".into(),
            "SHA256:one".into(),
            "b25l".into(),
        );
        assert!(repo.save(&first).is_ok());
        let mut second = first.clone();
        second.fingerprint = "SHA256:two".into();
        second.key_blob_base64 = "dHdv".into();
        second.updated_at_ms = second.created_at_ms + 1;
        assert!(repo.save(&second).is_ok());
        let loaded = repo
            .get("lab.example", 22)
            .unwrap_or_else(|error| unreachable!("get: {error}"))
            .unwrap_or_else(|| unreachable!("record must exist"));
        assert_eq!(loaded.fingerprint, "SHA256:two");
        assert_eq!(loaded.key_blob_base64, "dHdv");
        assert_eq!(repo.list().map(|keys| keys.len()).ok(), Some(1));
    }

    #[test]
    fn host_scoped_identity_round_trip() {
        let database = Database::open_in_memory()
            .unwrap_or_else(|error| unreachable!("in-memory database: {error}"));
        let repo = HostKeyRepository::new(database.connection());
        let host_id = HostId::new();
        assert!(database
            .connection()
            .execute(
                "INSERT INTO hosts (id, label, username, port) VALUES (?1, ?2, ?3, ?4)",
                params![
                    host_id.as_uuid().as_bytes().to_vec(),
                    "host-key-test",
                    "tester",
                    22
                ],
            )
            .is_ok());
        let record = HostKeyIdentityRecord {
            host_id,
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:host-scoped".into(),
            key_blob_base64: "aG9zdC1zY29wZWQ=".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        assert!(repo.save_for_host(&record).is_ok());
        assert_eq!(
            repo.get_for_host(record.host_id, "ssh-ed25519")
                .ok()
                .flatten(),
            Some(record)
        );
    }

    #[test]
    fn host_scoped_identity_keeps_algorithms_separate() {
        let database = Database::open_in_memory()
            .unwrap_or_else(|error| unreachable!("in-memory database: {error}"));
        let repo = HostKeyRepository::new(database.connection());
        let host_id = HostId::new();
        assert!(database
            .connection()
            .execute(
                "INSERT INTO hosts (id, label, username, port) VALUES (?1, ?2, ?3, ?4)",
                params![
                    host_id.as_uuid().as_bytes().to_vec(),
                    "multi-algorithm",
                    "tester",
                    22
                ],
            )
            .is_ok());
        let ed = HostKeyIdentityRecord::new(
            host_id,
            "ssh-ed25519".into(),
            "SHA256:ed".into(),
            "ZWQ=".into(),
        );
        let ecdsa = HostKeyIdentityRecord::new(
            host_id,
            "ecdsa-sha2-nistp256".into(),
            "SHA256:ecdsa".into(),
            "ZWNkc2E=".into(),
        );
        assert!(repo.save_for_host(&ed).is_ok());
        assert!(repo.save_for_host(&ecdsa).is_ok());
        assert_eq!(
            repo.get_for_host(host_id, "ssh-ed25519").ok().flatten(),
            Some(ed)
        );
        assert_eq!(
            repo.get_for_host(host_id, "ecdsa-sha2-nistp256")
                .ok()
                .flatten(),
            Some(ecdsa)
        );
    }
}
