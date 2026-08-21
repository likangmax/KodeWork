#![forbid(unsafe_code)]

use kodework_domain::{Address, AddressId, AuthenticationMode, Host, HostId, RuntimeKind};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

pub mod host_keys;
pub mod repositories;

pub const SCHEMA_VERSION: u32 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_domain_schema",
        sql: "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL);\nCREATE TABLE IF NOT EXISTS hosts (id BLOB PRIMARY KEY, label TEXT NOT NULL, username TEXT NOT NULL, port INTEGER NOT NULL, auth_ref TEXT);\nCREATE TABLE IF NOT EXISTS addresses (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, kind TEXT NOT NULL, hostname_or_ip TEXT NOT NULL, port INTEGER NOT NULL, priority INTEGER NOT NULL, enabled INTEGER NOT NULL);\nCREATE TABLE IF NOT EXISTS projects (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, name TEXT NOT NULL, remote_cwd TEXT NOT NULL, preferred_runtime TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS actions (id BLOB PRIMARY KEY, project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE, name TEXT NOT NULL, command TEXT NOT NULL, mode TEXT NOT NULL, cwd TEXT, timeout_ms INTEGER, danger_level TEXT NOT NULL, confirmation TEXT NOT NULL, env_json TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS runs (id BLOB PRIMARY KEY, action_id BLOB NOT NULL REFERENCES actions(id) ON DELETE CASCADE, status TEXT NOT NULL, started_at_ms INTEGER, finished_at_ms INTEGER, exit_code INTEGER, remote_session_ref TEXT, output_bytes INTEGER NOT NULL);\nCREATE TABLE IF NOT EXISTS sessions (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, project_id BLOB REFERENCES projects(id) ON DELETE SET NULL, runtime TEXT NOT NULL, external_ref TEXT, state TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS transfers (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, local_path TEXT NOT NULL, remote_path TEXT NOT NULL, direction TEXT NOT NULL, total_bytes INTEGER, transferred_bytes INTEGER NOT NULL, status TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);",
    },
    Migration {
        version: 2,
        name: "host_tailscale_configuration",
        sql: "ALTER TABLE hosts ADD COLUMN tailscale_json TEXT;",
    },
    Migration {
        version: 3,
        name: "host_default_runtime",
        sql: "ALTER TABLE hosts ADD COLUMN default_runtime TEXT NOT NULL DEFAULT 'Tmux';",
    },

    Migration {
        version: 4,
        name: "host_keys_tunnels_snippets_snapshots",
        sql: "CREATE TABLE IF NOT EXISTS host_keys (hostname TEXT NOT NULL, port INTEGER NOT NULL, algorithm TEXT NOT NULL, fingerprint TEXT NOT NULL, key_blob_base64 TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY (hostname, port));\nCREATE TABLE IF NOT EXISTS tunnels (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, remote_host TEXT NOT NULL, remote_port INTEGER NOT NULL, local_port INTEGER NOT NULL, status TEXT NOT NULL, session_id BLOB REFERENCES sessions(id) ON DELETE SET NULL);\nCREATE TABLE IF NOT EXISTS snippets (id BLOB PRIMARY KEY, project_id BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE, name TEXT NOT NULL, text TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0);\nCREATE TABLE IF NOT EXISTS workspace_snapshots (id BLOB PRIMARY KEY, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, payload_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL);",
    },    Migration {
        version: 5,
        name: "host_jump_configuration",
        sql: "ALTER TABLE hosts ADD COLUMN jump_hostname TEXT;\nALTER TABLE hosts ADD COLUMN jump_port INTEGER;\nALTER TABLE hosts ADD COLUMN jump_username TEXT;",
    },
    Migration {
        version: 6,
        name: "global_snippets",
        sql: "ALTER TABLE snippets RENAME TO snippets_v5;\nCREATE TABLE snippets (id BLOB PRIMARY KEY, name TEXT NOT NULL, text TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0);\nINSERT INTO snippets (id, name, text, sort_order) SELECT id, name, text, sort_order FROM snippets_v5;\nDROP TABLE snippets_v5;",
    },
    Migration {
        version: 7,
        name: "host_authentication_configuration",
        sql: "ALTER TABLE hosts ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'Password';\nALTER TABLE hosts ADD COLUMN private_key_path TEXT;",
    },
    Migration {
        version: 8,
        name: "host_default_remote_path",
        sql: "ALTER TABLE hosts ADD COLUMN default_remote_path TEXT NOT NULL DEFAULT '/';",
    },
    Migration {
        version: 9,
        name: "durable_run_history",
        sql: "CREATE TABLE runs_v9 (id BLOB PRIMARY KEY, action_id BLOB REFERENCES actions(id) ON DELETE SET NULL, host_id BLOB NOT NULL REFERENCES hosts(id) ON DELETE CASCADE, project_id BLOB REFERENCES projects(id) ON DELETE SET NULL, action_name TEXT NOT NULL, command_snapshot TEXT NOT NULL, mode TEXT NOT NULL, cwd_snapshot TEXT, status TEXT NOT NULL, started_at_ms INTEGER, finished_at_ms INTEGER, exit_code INTEGER, remote_session_ref TEXT, stdout_preview TEXT NOT NULL DEFAULT '', stderr_preview TEXT NOT NULL DEFAULT '', output_bytes INTEGER NOT NULL DEFAULT 0, last_reconciled_at_ms INTEGER);\nINSERT INTO runs_v9 (id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms) SELECT runs.id, runs.action_id, projects.host_id, actions.project_id, actions.name, actions.command, actions.mode, actions.cwd, runs.status, runs.started_at_ms, runs.finished_at_ms, runs.exit_code, runs.remote_session_ref, '', '', runs.output_bytes, NULL FROM runs INNER JOIN actions ON actions.id = runs.action_id INNER JOIN projects ON projects.id = actions.project_id;\nDROP TABLE runs;\nALTER TABLE runs_v9 RENAME TO runs;\nCREATE INDEX idx_runs_host_started ON runs(host_id, started_at_ms DESC);\nCREATE INDEX idx_runs_action_started ON runs(action_id, started_at_ms DESC);\nCREATE INDEX idx_runs_status ON runs(status);",
    },
    Migration {
        version: 10,
        name: "host_scoped_host_key_identities",
        sql: "CREATE TABLE host_key_identities (host_id BLOB PRIMARY KEY REFERENCES hosts(id) ON DELETE CASCADE, algorithm TEXT NOT NULL, fingerprint TEXT NOT NULL, key_blob_base64 TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);",
    },
    Migration {
        version: 11,
        name: "remove_persisted_run_output_previews",
        sql: "UPDATE runs SET stdout_preview = '', stderr_preview = '';",
    },
];

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("unsupported migration version {0}")]
    UnsupportedVersion(u32),
    #[error("migration list is not strictly increasing")]
    InvalidMigrationOrder,
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid UUID bytes in storage")]
    InvalidId,
    #[error("run not found: {0:?}")]
    RunNotFound(kodework_domain::RunId),
    #[error("invalid run status transition: {0:?} -> {1:?}")]
    InvalidRunTransition(kodework_domain::RunStatus, kodework_domain::RunStatus),
}

pub fn validate_migrations() -> Result<(), StorageError> {
    let mut previous = 0;
    for migration in MIGRATIONS {
        if migration.version <= previous {
            return Err(StorageError::InvalidMigrationOrder);
        }
        previous = migration.version;
    }
    if previous != SCHEMA_VERSION {
        return Err(StorageError::UnsupportedVersion(previous));
    }
    Ok(())
}

pub struct Database {
    connection: Connection,
}

impl Database {
    /// Borrows the raw SQLite connection (repository access).
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        let database = Self { connection };
        database.configure()?;
        database.apply_migrations()?;
        Ok(database)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        let database = Self { connection };
        database.configure()?;
        database.apply_migrations()?;
        Ok(database)
    }

    fn configure(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;",
        )?;
        Ok(())
    }

    fn apply_migrations(&self) -> Result<(), StorageError> {
        validate_migrations()?;
        let applied: Vec<u32> = if table_exists(&self.connection, "schema_migrations")? {
            let mut statement = self
                .connection
                .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = statement.query_map([], |row| row.get::<_, u32>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let transaction = self.connection.unchecked_transaction()?;
        for migration in MIGRATIONS {
            if applied.contains(&migration.version) {
                continue;
            }
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (?1, ?2)",
                params![migration.version, now_millis()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32, StorageError> {
        let version = self
            .connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<u32>>(0)
            })?
            .unwrap_or(0);
        Ok(version)
    }

    pub fn upsert_host(&self, host: &Host) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO hosts (id, label, username, port, auth_ref, tailscale_json, default_runtime, jump_hostname, jump_port, jump_username, auth_mode, private_key_path, default_remote_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT(id) DO UPDATE SET label = excluded.label, username = excluded.username, port = excluded.port, auth_ref = excluded.auth_ref, tailscale_json = excluded.tailscale_json, default_runtime = excluded.default_runtime, jump_hostname = excluded.jump_hostname, jump_port = excluded.jump_port, jump_username = excluded.jump_username, auth_mode = excluded.auth_mode, private_key_path = excluded.private_key_path, default_remote_path = excluded.default_remote_path",
            params![
                host.id.as_uuid().as_bytes().to_vec(),
                host.label,
                host.username,
                host.port,
                optional_json(&host.auth_ref)?,
                optional_json(&host.tailscale)?,
                serde_json::to_string(&host.default_runtime)?,
                host.jump.as_ref().map(|jump| jump.hostname.as_str()),
                host.jump.as_ref().map(|jump| jump.port),
                host.jump.as_ref().map(|jump| jump.username.as_str()),
                serde_json::to_string(&host.auth_mode)?,
                host.private_key_path.as_deref(),
                host.default_remote_path,
            ],
        )?;
        transaction.execute(
            "DELETE FROM addresses WHERE host_id = ?1",
            params![host.id.as_uuid().as_bytes().to_vec()],
        )?;
        for address in &host.addresses {
            transaction.execute(
                "INSERT INTO addresses (id, host_id, kind, hostname_or_ip, port, priority, enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    address.id.as_uuid().as_bytes().to_vec(),
                    host.id.as_uuid().as_bytes().to_vec(),
                    serde_json::to_string(&address.kind)?,
                    address.hostname_or_ip,
                    address.port,
                    address.priority,
                    address.enabled,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn get_host(&self, id: HostId) -> Result<Option<Host>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, label, username, port, auth_ref, tailscale_json, default_runtime, jump_hostname, jump_port, jump_username, auth_mode, private_key_path, default_remote_path FROM hosts WHERE id = ?1",
        )?;
        let row = statement
            .query_row(params![id.as_uuid().as_bytes().to_vec()], read_host_row)
            .optional()?;
        row.map(|host| self.load_addresses(host)).transpose()
    }

    pub fn list_hosts(&self) -> Result<Vec<Host>, StorageError> {
        let mut statement = self.connection.prepare("SELECT id, label, username, port, auth_ref, tailscale_json, default_runtime, jump_hostname, jump_port, jump_username, auth_mode, private_key_path, default_remote_path FROM hosts ORDER BY label COLLATE NOCASE")?;
        let rows = statement.query_map([], read_host_row)?;
        let base_hosts = rows.collect::<Result<Vec<_>, _>>()?;
        base_hosts
            .into_iter()
            .map(|host| self.load_addresses(host))
            .collect()
    }

    pub fn delete_host(&self, id: HostId) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM hosts WHERE id = ?1",
            params![id.as_uuid().as_bytes().to_vec()],
        )?;
        Ok(changed != 0)
    }

    fn load_addresses(&self, mut host: Host) -> Result<Host, StorageError> {
        let mut statement = self.connection.prepare("SELECT id, kind, hostname_or_ip, port, priority, enabled FROM addresses WHERE host_id = ?1 ORDER BY priority, hostname_or_ip")?;
        let rows = statement.query_map(params![host.id.as_uuid().as_bytes().to_vec()], |row| {
            let id = uuid_from_blob(row.get::<_, Vec<u8>>(0)?)?;
            let kind_json = row.get::<_, String>(1)?;
            let kind = serde_json::from_str(&kind_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(Address {
                id: AddressId::from_uuid(id),
                kind,
                hostname_or_ip: row.get(2)?,
                port: row.get(3)?,
                priority: row.get(4)?,
                enabled: row.get(5)?,
            })
        })?;
        host.addresses = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(host)
    }
}

fn read_host_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Host> {
    let id = uuid_from_blob(row.get::<_, Vec<u8>>(0)?)?;
    let auth_ref = optional_from_json(row.get::<_, Option<String>>(4)?)?;
    let tailscale = optional_from_json(row.get::<_, Option<String>>(5)?)?;
    let runtime_json = row.get::<_, String>(6)?;
    // v3 seeded a bare 'Tmux' default (not JSON) for pre-existing rows;
    // tolerate both the serde format written today and the legacy seed.
    let default_runtime = match serde_json::from_str::<RuntimeKind>(&runtime_json) {
        Ok(runtime) => runtime,
        Err(_) => match runtime_json.as_str() {
            "Herdr" => RuntimeKind::Herdr,
            "PlainShell" => RuntimeKind::PlainShell,
            _ => RuntimeKind::Tmux,
        },
    };
    let jump_hostname = row.get::<_, Option<String>>(7)?;
    let jump_port = row.get::<_, Option<u16>>(8)?;
    let jump_username = row.get::<_, Option<String>>(9)?;
    let jump = match (jump_hostname, jump_port, jump_username) {
        (Some(hostname), Some(port), Some(username)) => Some(kodework_domain::JumpHost {
            hostname,
            port,
            username,
        }),
        _ => None,
    };
    let auth_mode_json = row.get::<_, String>(10)?;
    // Credential semantics must fail closed. Treating corrupted or unknown
    // data as Password can send the wrong secret through the wrong SSH method.
    let auth_mode = match serde_json::from_str::<AuthenticationMode>(&auth_mode_json) {
        Ok(mode) => mode,
        Err(error) => match auth_mode_json.as_str() {
            // v7 seeded this field with a bare enum name. Accept only the
            // finite legacy spellings; every other malformed value fails
            // closed instead of silently changing credential semantics.
            "Password" => AuthenticationMode::Password,
            "PublicKey" => AuthenticationMode::PublicKey,
            "SshAgent" => AuthenticationMode::SshAgent,
            "KeyboardInteractive" => AuthenticationMode::KeyboardInteractive,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                ));
            }
        },
    };
    Ok(Host {
        id: HostId::from_uuid(id),
        label: row.get(1)?,
        username: row.get(2)?,
        port: row.get(3)?,
        auth_ref,
        auth_mode,
        private_key_path: row.get(11)?,
        default_remote_path: row.get(12)?,
        addresses: Vec::new(),
        tailscale,
        default_runtime,
        jump,
    })
}

fn uuid_from_blob(value: Vec<u8>) -> rusqlite::Result<Uuid> {
    Uuid::from_slice(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error))
    })
}

fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>, StorageError> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(StorageError::from)
}

fn optional_from_json<T: DeserializeOwned>(value: Option<String>) -> rusqlite::Result<Option<T>> {
    value
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get::<_, i32>(0),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

pub(crate) fn now_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{
        AddressKind, CredentialProvider, CredentialRef, RunId, RuntimeKind, TailscaleConfig,
        TailscaleMode,
    };

    fn fixture_host() -> Host {
        Host {
            id: HostId::new(),
            label: "Example private host".into(),
            username: "tester".into(),
            port: 22,
            auth_ref: Some(CredentialRef {
                provider: CredentialProvider::Test,
                opaque_id: "fixture-password".into(),
            }),
            auth_mode: AuthenticationMode::Password,
            private_key_path: None,
            default_remote_path: "/workspace".into(),
            jump: None,
            addresses: vec![Address {
                id: AddressId::new(),
                kind: AddressKind::Tailscale,
                hostname_or_ip: "203.0.113.10".into(),
                port: 22,
                priority: 1,
                enabled: true,
            }],
            tailscale: Some(TailscaleConfig {
                enabled: true,
                mode: TailscaleMode::EmbeddedUserspace,
                device_name: Some("fixture-device".into()),
                auth_key_ref: Some(CredentialRef {
                    provider: CredentialProvider::Test,
                    opaque_id: "fixture-tailscale-key".into(),
                }),
                state_dir: None,
            }),
            default_runtime: RuntimeKind::Herdr,
        }
    }

    #[test]
    fn migrations_are_valid_and_idempotent() {
        let database = Database::open_in_memory();
        assert!(database.is_ok());
        let database = database.unwrap_or_else(|_| unreachable!("checked above"));
        assert_eq!(database.schema_version().ok(), Some(SCHEMA_VERSION));
        assert!(database.apply_migrations().is_ok());
        assert_eq!(database.schema_version().ok(), Some(SCHEMA_VERSION));
    }

    #[test]
    fn host_round_trip_preserves_addresses_and_tailscale_reference() {
        let database =
            Database::open_in_memory().unwrap_or_else(|_| unreachable!("in-memory database"));
        let host = fixture_host();
        assert!(database.upsert_host(&host).is_ok());
        let loaded = database
            .get_host(host.id)
            .unwrap_or_else(|_| unreachable!("host query"))
            .unwrap_or_else(|| unreachable!("host was inserted"));
        assert_eq!(loaded, host);
        assert_eq!(database.list_hosts().map(|hosts| hosts.len()).ok(), Some(1));
        assert_eq!(database.delete_host(host.id).ok(), Some(true));
        assert_eq!(database.get_host(host.id).ok().flatten(), None);
    }

    #[test]
    fn migration_plan_is_strictly_ordered() {
        assert!(validate_migrations().is_ok());
    }

    #[test]
    fn migration_11_clears_legacy_run_output_previews() {
        let connection = rusqlite::Connection::open_in_memory()
            .unwrap_or_else(|error| unreachable!("open migration fixture: {error}"));
        for migration in &MIGRATIONS[..10] {
            connection
                .execute_batch(migration.sql)
                .unwrap_or_else(|error| {
                    unreachable!("apply migration {}: {error}", migration.version)
                });
        }
        let host_id = HostId::new();
        connection
            .execute(
                "INSERT INTO hosts (id, label, username, port, auth_ref, tailscale_json, default_runtime, jump_hostname, jump_port, jump_username, auth_mode, private_key_path, default_remote_path)
                 VALUES (?1, 'legacy host', 'tester', 22, NULL, NULL, 'Tmux', NULL, NULL, NULL, 'Password', NULL, '/')",
                rusqlite::params![host_id.as_uuid().as_bytes()],
            )
            .unwrap_or_else(|error| unreachable!("insert legacy host: {error}"));
        let run_id = RunId::new();
        connection
            .execute(
                "INSERT INTO runs (id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms)
                 VALUES (?1, NULL, ?2, NULL, 'legacy', 'echo legacy', 'Background', NULL, 'Running', NULL, NULL, NULL, NULL, ?3, ?4, 7, NULL)",
                rusqlite::params![
                    run_id.as_uuid().as_bytes(),
                    host_id.as_uuid().as_bytes(),
                    "legacy stdout secret",
                    "legacy stderr secret",
                ],
            )
            .unwrap_or_else(|error| unreachable!("insert legacy run: {error}"));

        connection
            .execute_batch(MIGRATIONS[10].sql)
            .unwrap_or_else(|error| unreachable!("apply migration 11: {error}"));
        let previews: (String, String) = connection
            .query_row(
                "SELECT stdout_preview, stderr_preview FROM runs WHERE id = ?1",
                rusqlite::params![run_id.as_uuid().as_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or_else(|error| unreachable!("read migrated run: {error}"));
        assert_eq!(previews, (String::new(), String::new()));
    }

    #[test]
    fn migration_from_v1_preserves_existing_hosts() {
        // Build a real v1 database by hand, insert a host, then open it
        // through the normal path: every migration up to the current
        // schema must run and the pre-existing host must survive.
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("tempdir: {error}"));
        let path = dir.path().join("legacy.sqlite3");
        {
            let connection = rusqlite::Connection::open(&path)
                .unwrap_or_else(|error| unreachable!("open legacy db: {error}"));
            connection
                .execute_batch(MIGRATIONS[0].sql)
                .unwrap_or_else(|error| unreachable!("apply v1: {error}"));
            let host = fixture_host();
            connection
                .execute(
                    "INSERT INTO hosts (id, label, username, port, auth_ref)
                     VALUES (?1, ?2, ?3, ?4, NULL)",
                    rusqlite::params![
                        host.id.as_uuid().as_bytes(),
                        host.label,
                        host.username,
                        host.port,
                    ],
                )
                .unwrap_or_else(|error| unreachable!("insert v1 host: {error}"));
        }
        let database =
            Database::open(path).unwrap_or_else(|error| unreachable!("open migrated db: {error}"));
        assert_eq!(database.schema_version().ok(), Some(SCHEMA_VERSION));
        let hosts = database
            .list_hosts()
            .unwrap_or_else(|error| unreachable!("list: {error}"));
        assert_eq!(hosts.len(), 1, "pre-existing host must survive migrations");
        assert_eq!(hosts[0].label, fixture_host().label);
        assert_eq!(hosts[0].default_remote_path, "/");
    }
}
