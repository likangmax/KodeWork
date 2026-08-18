#![forbid(unsafe_code)]

//! Host-key persistence (metadata only; keys are not secrets).

use crate::{now_millis, StorageError};
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
}
