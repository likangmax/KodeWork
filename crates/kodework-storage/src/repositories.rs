#![forbid(unsafe_code)]

//! Repositories for project-scoped and session-scoped records. All tables
//! store metadata only; credential material never appears here.

use crate::StorageError;
use kodework_domain::{
    Action, ActionId, HostId, Project, ProjectId, Run, RunId, RunStatus, Session, SessionId,
    SessionState, Snippet, SnippetId, Transfer, TransferId, TransferStatus, Tunnel, TunnelId,
};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

macro_rules! id_to_blob {
    ($id:expr) => {
        $id.as_uuid().as_bytes().to_vec()
    };
}

macro_rules! blob_to_id {
    ($row:expr, $index:expr, $ty:ty) => {{
        let bytes: Vec<u8> = $row.get($index)?;
        let uuid = Uuid::from_slice(&bytes).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                $index,
                rusqlite::types::Type::Blob,
                Box::new(error),
            )
        })?;
        <$ty>::from_uuid(uuid)
    }};
}

fn json_from<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    Ok(serde_json::to_string(value)?)
}

/// Projects: one per host.
pub struct ProjectRepository<'a> {
    connection: &'a Connection,
}

impl<'a> ProjectRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_host(&self, host_id: HostId) -> Result<Vec<Project>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, host_id, name, remote_cwd, preferred_runtime FROM projects WHERE host_id = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![id_to_blob!(host_id)], read_project)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: ProjectId) -> Result<Option<Project>, StorageError> {
        let project = self
            .connection
            .query_row(
                "SELECT id, host_id, name, remote_cwd, preferred_runtime FROM projects WHERE id = ?1",
                params![id_to_blob!(id)],
                read_project,
            )
            .optional()?;
        Ok(project)
    }

    pub fn upsert(&self, project: &Project) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO projects (id, host_id, name, remote_cwd, preferred_runtime)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                host_id = excluded.host_id,
                name = excluded.name,
                remote_cwd = excluded.remote_cwd,
                preferred_runtime = excluded.preferred_runtime",
            params![
                id_to_blob!(project.id),
                id_to_blob!(project.host_id),
                project.name,
                project.remote_cwd,
                json_from(&project.preferred_runtime)?,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: ProjectId) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM projects WHERE id = ?1",
            params![id_to_blob!(id)],
        )?;
        Ok(changed != 0)
    }
}

fn read_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: blob_to_id!(row, 0, ProjectId),
        host_id: blob_to_id!(row, 1, HostId),
        name: row.get(2)?,
        remote_cwd: row.get(3)?,
        preferred_runtime: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

/// Actions: commands attached to a project.
pub struct ActionRepository<'a> {
    connection: &'a Connection,
}

impl<'a> ActionRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<Action>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, name, command, mode, cwd, timeout_ms, danger_level, confirmation, env_json
             FROM actions WHERE project_id = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![id_to_blob!(project_id)], read_action)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        let action = self
            .connection
            .query_row(
                "SELECT id, project_id, name, command, mode, cwd, timeout_ms, danger_level, confirmation, env_json
                 FROM actions WHERE id = ?1",
                params![id_to_blob!(id)],
                read_action,
            )
            .optional()?;
        Ok(action)
    }

    pub fn upsert(&self, action: &Action) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO actions (id, project_id, name, command, mode, cwd, timeout_ms, danger_level, confirmation, env_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                name = excluded.name,
                command = excluded.command,
                mode = excluded.mode,
                cwd = excluded.cwd,
                timeout_ms = excluded.timeout_ms,
                danger_level = excluded.danger_level,
                confirmation = excluded.confirmation,
                env_json = excluded.env_json",
            params![
                id_to_blob!(action.id),
                id_to_blob!(action.project_id),
                action.name,
                action.command,
                json_from(&action.mode)?,
                action.cwd,
                action.timeout_ms,
                json_from(&action.danger_level)?,
                json_from(&action.confirmation)?,
                json_from(&action.env)?,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM actions WHERE id = ?1",
            params![id_to_blob!(id)],
        )?;
        Ok(changed != 0)
    }
}

fn read_action(row: &rusqlite::Row<'_>) -> rusqlite::Result<Action> {
    Ok(Action {
        id: blob_to_id!(row, 0, ActionId),
        project_id: blob_to_id!(row, 1, ProjectId),

        name: row.get(2)?,
        command: row.get(3)?,
        mode: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        cwd: row.get(5)?,
        timeout_ms: row.get(6)?,
        danger_level: serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        confirmation: serde_json::from_str(&row.get::<_, String>(8)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        env: serde_json::from_str(&row.get::<_, String>(9)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}
/// Runs: durable lifecycle records for actions.
///
/// Command output is intentionally ephemeral. The native command may return a
/// bounded preview to the active renderer, but this repository never writes
/// stdout/stderr previews to SQLite because remote output can contain secrets.
pub struct RunRepository<'a> {
    connection: &'a Connection,
}

impl<'a> RunRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, run: &Run) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO runs (id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                id_to_blob!(run.id),
                run.action_id.map(|id| id_to_blob!(id)),
                id_to_blob!(run.host_id),
                run.project_id.map(|id| id_to_blob!(id)),
                run.action_name,
                run.command_snapshot,
                json_from(&run.mode)?,
                run.cwd_snapshot,
                json_from(&run.status)?,
                run.started_at_ms,
                run.finished_at_ms,
                run.exit_code,
                run.remote_session_ref,
                "",
                "",
                run.output_bytes,
                run.last_reconciled_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: RunId) -> Result<Option<Run>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms FROM runs WHERE id = ?1",
                params![id_to_blob!(id)],
                read_run,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn list_reconcilable_by_host(
        &self,
        host_id: HostId,
        limit: usize,
    ) -> Result<Vec<Run>, StorageError> {
        let statuses = [
            json_from(&RunStatus::Queued)?,
            json_from(&RunStatus::Running)?,
            json_from(&RunStatus::Unknown)?,
        ];
        let mut statement = self.connection.prepare(
            "SELECT id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms FROM runs WHERE host_id = ?1 AND mode = ?2 AND status IN (?3, ?4, ?5) ORDER BY COALESCE(started_at_ms, 0) DESC LIMIT ?6",
        )?;
        let rows = statement.query_map(
            params![
                id_to_blob!(host_id),
                json_from(&kodework_domain::ActionMode::Background)?,
                statuses[0],
                statuses[1],
                statuses[2],
                limit.min(500) as i64
            ],
            read_run,
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn update_status(
        &self,
        id: RunId,
        status: RunStatus,
        exit_code: Option<i32>,
        finished_at_ms: Option<u64>,
    ) -> Result<(), StorageError> {
        let current = self.current_status(id)?;
        if !kodework_domain::run_transition(current, status) {
            return Err(StorageError::InvalidRunTransition(current, status));
        }
        self.connection.execute(
            "UPDATE runs SET status = ?1, exit_code = ?2, finished_at_ms = ?3 WHERE id = ?4",
            params![
                json_from(&status)?,
                exit_code,
                finished_at_ms,
                id_to_blob!(id)
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &self,
        id: RunId,
        status: RunStatus,
        exit_code: Option<i32>,
        finished_at_ms: Option<u64>,
        remote_session_ref: Option<&str>,
        output_bytes: u64,
        _stdout_preview: &str,
        _stderr_preview: &str,
        reconciled_at_ms: Option<u64>,
    ) -> Result<(), StorageError> {
        let current = self.current_status(id)?;
        if !kodework_domain::run_transition(current, status) {
            return Err(StorageError::InvalidRunTransition(current, status));
        }
        self.connection.execute(
            "UPDATE runs SET status = ?1, exit_code = ?2, finished_at_ms = ?3, remote_session_ref = ?4, output_bytes = ?5, stdout_preview = ?6, stderr_preview = ?7, last_reconciled_at_ms = ?8 WHERE id = ?9",
            params![
                json_from(&status)?,
                exit_code,
                finished_at_ms,
                remote_session_ref,
                output_bytes,
                "",
                "",
                reconciled_at_ms,
                id_to_blob!(id)
            ],
        )?;
        Ok(())
    }

    fn current_status(&self, id: RunId) -> Result<RunStatus, StorageError> {
        let raw = self
            .connection
            .query_row(
                "SELECT status FROM runs WHERE id = ?1",
                params![id_to_blob!(id)],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(StorageError::RunNotFound(id))?;
        serde_json::from_str(&raw).map_err(StorageError::Serialization)
    }

    pub fn list_by_action(
        &self,
        action_id: ActionId,
        limit: usize,
    ) -> Result<Vec<Run>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms
             FROM runs WHERE action_id = ?1 ORDER BY started_at_ms DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![id_to_blob!(action_id), limit as i64], read_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Returns the newest runs across all actions with a bounded result set.
    pub fn list_recent(&self, limit: usize) -> Result<Vec<Run>, StorageError> {
        let limit = limit.min(500);
        let mut statement = self.connection.prepare(
            "SELECT id, action_id, host_id, project_id, action_name, command_snapshot, mode, cwd_snapshot, status, started_at_ms, finished_at_ms, exit_code, remote_session_ref, stdout_preview, stderr_preview, output_bytes, last_reconciled_at_ms
             FROM runs ORDER BY COALESCE(started_at_ms, 0) DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], read_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Returns the newest runs belonging to projects on one host. Activity
    /// views are host-scoped; a global query would leak unrelated workspace
    /// history into the selected Host's UI.
    pub fn list_recent_by_host(
        &self,
        host_id: HostId,
        limit: usize,
    ) -> Result<Vec<Run>, StorageError> {
        let limit = limit.min(500);
        let mut statement = self.connection.prepare(
            "SELECT runs.id, runs.action_id, runs.host_id, runs.project_id, runs.action_name,
                    runs.command_snapshot, runs.mode, runs.cwd_snapshot, runs.status,
                    runs.started_at_ms, runs.finished_at_ms, runs.exit_code,
                    runs.remote_session_ref, runs.stdout_preview, runs.stderr_preview,
                    runs.output_bytes, runs.last_reconciled_at_ms
             FROM runs
             WHERE runs.host_id = ?1
             ORDER BY COALESCE(runs.started_at_ms, 0) DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![id_to_blob!(host_id), limit as i64], read_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn read_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: blob_to_id!(row, 0, RunId),
        action_id: row
            .get::<_, Option<Vec<u8>>>(1)?
            .map(|value| uuid_from_blob(value).map(ActionId::from_uuid))
            .transpose()?,
        host_id: HostId::from_uuid(uuid_from_blob(row.get(2)?)?),
        project_id: row
            .get::<_, Option<Vec<u8>>>(3)?
            .map(|value| uuid_from_blob(value).map(ProjectId::from_uuid))
            .transpose()?,
        action_name: row.get(4)?,
        command_snapshot: row.get(5)?,
        mode: serde_json::from_str(&row.get::<_, String>(6)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        cwd_snapshot: row.get(7)?,
        status: serde_json::from_str(&row.get::<_, String>(8)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        started_at_ms: row.get(9)?,
        finished_at_ms: row.get(10)?,
        exit_code: row.get(11)?,
        remote_session_ref: row.get(12)?,
        stdout_preview: row.get(13)?,
        stderr_preview: row.get(14)?,
        output_bytes: row.get(15)?,
        last_reconciled_at_ms: row.get(16)?,
    })
}

fn uuid_from_blob(value: Vec<u8>) -> rusqlite::Result<Uuid> {
    Uuid::from_slice(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error))
    })
}

/// Sessions: tmux/Herdr attachment records.
pub struct SessionRepository<'a> {
    connection: &'a Connection,
}

impl<'a> SessionRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, session: &Session) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO sessions (id, host_id, project_id, runtime, external_ref, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id_to_blob!(session.id),
                id_to_blob!(session.host_id),
                session.project_id.map(|id| id_to_blob!(id)),
                json_from(&session.runtime)?,
                session.external_ref,
                json_from(&session.state)?,
            ],
        )?;
        Ok(())
    }

    pub fn update_state(&self, id: SessionId, state: SessionState) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE sessions SET state = ?1 WHERE id = ?2",
            params![json_from(&state)?, id_to_blob!(id)],
        )?;
        Ok(())
    }

    pub fn list_by_host(&self, host_id: HostId) -> Result<Vec<Session>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, host_id, project_id, runtime, external_ref, state
             FROM sessions WHERE host_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![id_to_blob!(host_id)], read_session)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: SessionId) -> Result<Option<Session>, StorageError> {
        let session = self
            .connection
            .query_row(
                "SELECT id, host_id, project_id, runtime, external_ref, state FROM sessions WHERE id = ?1",
                params![id_to_blob!(id)],
                read_session,
            )
            .optional()?;
        Ok(session)
    }
}

fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let project_blob: Option<Vec<u8>> = row.get(2)?;
    let project_id = match project_blob {
        Some(bytes) => {
            let uuid = Uuid::from_slice(&bytes).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })?;
            Some(ProjectId::from_uuid(uuid))
        }
        None => None,
    };
    Ok(Session {
        id: blob_to_id!(row, 0, SessionId),
        host_id: blob_to_id!(row, 1, HostId),
        project_id,
        runtime: serde_json::from_str(&row.get::<_, String>(3)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        external_ref: row.get(4)?,
        state: serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

/// Transfers: SFTP transfer records with resumable progress.
pub struct TransferRepository<'a> {
    connection: &'a Connection,
}

impl<'a> TransferRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, transfer: &Transfer) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO transfers (id, host_id, local_path, remote_path, direction, total_bytes, transferred_bytes, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id_to_blob!(transfer.id),
                id_to_blob!(transfer.host_id),
                transfer.local_path,
                transfer.remote_path,
                json_from(&transfer.direction)?,
                transfer.total_bytes,
                transfer.transferred_bytes,
                json_from(&transfer.status)?,
            ],
        )?;
        Ok(())
    }

    pub fn update_progress(
        &self,
        id: TransferId,
        transferred_bytes: u64,
        status: TransferStatus,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE transfers SET transferred_bytes = ?1, status = ?2 WHERE id = ?3",
            params![transferred_bytes, json_from(&status)?, id_to_blob!(id)],
        )?;
        Ok(())
    }

    pub fn list_by_host(&self, host_id: HostId) -> Result<Vec<Transfer>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, host_id, local_path, remote_path, direction, total_bytes, transferred_bytes, status
             FROM transfers WHERE host_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![id_to_blob!(host_id)], read_transfer)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn read_transfer(row: &rusqlite::Row<'_>) -> rusqlite::Result<Transfer> {
    Ok(Transfer {
        id: blob_to_id!(row, 0, TransferId),
        host_id: blob_to_id!(row, 1, HostId),
        local_path: row.get(2)?,
        remote_path: row.get(3)?,
        direction: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        total_bytes: row.get(5)?,
        transferred_bytes: row.get(6)?,
        status: serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

/// Tunnels: SSH local port forwarding records.
pub struct TunnelRepository<'a> {
    connection: &'a Connection,
}

impl<'a> TunnelRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, tunnel: &Tunnel) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO tunnels (id, host_id, remote_host, remote_port, local_port, status, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id_to_blob!(tunnel.id),
                id_to_blob!(tunnel.host_id),
                tunnel.remote_host,
                tunnel.remote_port,
                tunnel.local_port,
                "open",
                Option::<Vec<u8>>::None,
            ],
        )?;
        Ok(())
    }

    pub fn list_by_host(&self, host_id: HostId) -> Result<Vec<Tunnel>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, host_id, remote_host, remote_port, local_port FROM tunnels WHERE host_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![id_to_blob!(host_id)], read_tunnel)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: TunnelId) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM tunnels WHERE id = ?1",
            params![id_to_blob!(id)],
        )?;
        Ok(changed != 0)
    }
}

fn read_tunnel(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tunnel> {
    Ok(Tunnel {
        id: blob_to_id!(row, 0, TunnelId),
        host_id: blob_to_id!(row, 1, HostId),
        remote_host: row.get(2)?,
        remote_port: row.get(3)?,
        local_port: row.get(4)?,
    })
}

/// Snippets: global paste-able command fragments.
pub struct SnippetRepository<'a> {
    connection: &'a Connection,
}

impl<'a> SnippetRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn list_all(&self) -> Result<Vec<Snippet>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, text, sort_order FROM snippets ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], read_snippet)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upsert(&self, snippet: &Snippet) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO snippets (id, name, text, sort_order)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                text = excluded.text,
                sort_order = excluded.sort_order",
            params![
                id_to_blob!(snippet.id),
                snippet.name,
                snippet.text,
                snippet.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: SnippetId) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM snippets WHERE id = ?1",
            params![id_to_blob!(id)],
        )?;
        Ok(changed != 0)
    }
}

fn read_snippet(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: blob_to_id!(row, 0, SnippetId),
        name: row.get(1)?,
        text: row.get(2)?,
        sort_order: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{now_millis, Database};
    use kodework_domain::{
        ActionMode, ConfirmationPolicy, DangerLevel, RuntimeKind, TransferDirection,
    };

    fn database() -> Database {
        Database::open_in_memory().unwrap_or_else(|error| unreachable!("db: {error}"))
    }

    fn seed_host(db: &Database) -> HostId {
        let host = kodework_domain::Host {
            id: HostId::new(),
            label: "test-host".into(),
            username: "tester".into(),
            port: 22,
            auth_ref: None,
            auth_mode: kodework_domain::AuthenticationMode::Password,
            private_key_path: None,
            default_remote_path: "/".into(),
            jump: None,
            addresses: Vec::new(),
            tailscale: None,
            default_runtime: RuntimeKind::Tmux,
        };
        db.upsert_host(&host).unwrap_or_else(|error| {
            unreachable!("host seed failed: {error:?}");
        });
        host.id
    }

    #[test]
    fn project_action_run_round_trip() {
        let db = database();
        let projects = ProjectRepository::new(db.connection());
        let project = Project {
            id: ProjectId::new(),
            host_id: seed_host(&db),
            name: "main".into(),
            remote_cwd: "~/code/main".into(),
            preferred_runtime: RuntimeKind::Herdr,
        };
        projects.upsert(&project).unwrap_or_else(|error| {
            unreachable!("project upsert failed: {error:?}");
        });
        let loaded = projects
            .get(project.id)
            .unwrap_or_else(|error| unreachable!("get: {error}"))
            .unwrap_or_else(|| unreachable!("project must exist"));
        assert_eq!(loaded, project);
        assert_eq!(
            projects
                .list_by_host(project.host_id)
                .map(|items| items.len())
                .ok(),
            Some(1)
        );

        let actions = ActionRepository::new(db.connection());
        let action = Action {
            id: ActionId::new(),
            project_id: project.id,
            name: "test".into(),
            command: "cargo test".into(),
            mode: ActionMode::Quick,
            cwd: None,
            timeout_ms: Some(30_000),
            danger_level: DangerLevel::Safe,
            confirmation: ConfirmationPolicy::Never,
            env: std::collections::BTreeMap::new(),
        };
        assert!(actions.upsert(&action).is_ok());
        let loaded_action = actions
            .get(action.id)
            .unwrap_or_else(|error| unreachable!("get action: {error}"))
            .unwrap_or_else(|| unreachable!("action must exist"));
        assert_eq!(loaded_action, action);

        let runs = RunRepository::new(db.connection());
        let run = Run {
            id: RunId::new(),
            action_id: Some(action.id),
            host_id: project.host_id,
            project_id: Some(project.id),
            action_name: action.name.clone(),
            command_snapshot: action.command.clone(),
            mode: action.mode,
            cwd_snapshot: action.cwd.clone(),
            status: RunStatus::Running,
            started_at_ms: Some(now_millis() as u64),
            finished_at_ms: None,
            exit_code: None,
            remote_session_ref: None,
            stdout_preview: "secret-from-remote-output".into(),
            stderr_preview: "another-secret".into(),
            output_bytes: 0,
            last_reconciled_at_ms: None,
        };
        assert!(runs.create(&run).is_ok());
        assert!(runs
            .finish(
                run.id,
                RunStatus::Succeeded,
                Some(0),
                Some(1),
                Some("tmux:kodework-run-test"),
                4096,
                "ok",
                "",
                Some(now_millis() as u64),
            )
            .is_ok());
        let runs = runs
            .list_by_action(action.id, 10)
            .unwrap_or_else(|error| unreachable!("list runs: {error}"));
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, RunStatus::Succeeded);
        assert_eq!(runs[0].exit_code, Some(0));
        assert_eq!(
            runs[0].remote_session_ref.as_deref(),
            Some("tmux:kodework-run-test")
        );
        assert_eq!(runs[0].output_bytes, 4096);
        assert_eq!(
            runs[0].stdout_preview, "",
            "run output previews must never be persisted"
        );
        assert_eq!(runs[0].stderr_preview, "");
        assert_eq!(
            RunRepository::new(db.connection())
                .list_recent_by_host(project.host_id, 10)
                .unwrap_or_else(|error| unreachable!("list host runs: {error}"))
                .len(),
            1
        );

        assert!(actions.delete(action.id).ok() == Some(true));
        assert!(projects.delete(project.id).ok() == Some(true));
        assert!(matches!(
            RunRepository::new(db.connection()).finish(
                run.id,
                RunStatus::Running,
                None,
                None,
                None,
                0,
                "",
                "",
                None,
            ),
            Err(StorageError::InvalidRunTransition(
                RunStatus::Succeeded,
                RunStatus::Running
            ))
        ));
    }

    #[test]
    fn session_transfer_tunnel_snippet_round_trip() {
        let db = database();
        let host_id = seed_host(&db);

        let sessions = SessionRepository::new(db.connection());
        let session = Session {
            id: SessionId::new(),
            host_id,
            project_id: None,
            runtime: RuntimeKind::Tmux,
            external_ref: Some("tmux:0".into()),
            state: SessionState::Attached,
        };
        sessions.create(&session).unwrap_or_else(|error| {
            unreachable!("session create failed: {error:?}");
        });
        assert!(sessions
            .update_state(session.id, SessionState::Suspended)
            .is_ok());
        let loaded = sessions
            .get(session.id)
            .unwrap_or_else(|error| unreachable!("get session: {error}"))
            .unwrap_or_else(|| unreachable!("session must exist"));
        assert_eq!(loaded.state, SessionState::Suspended);
        assert_eq!(loaded.external_ref.as_deref(), Some("tmux:0"));

        let transfers = TransferRepository::new(db.connection());
        let transfer = Transfer {
            id: TransferId::new(),
            host_id,
            local_path: "C:/tmp/a.bin".into(),
            remote_path: "~/a.bin".into(),
            direction: TransferDirection::Upload,
            total_bytes: Some(1024),
            transferred_bytes: 512,
            status: TransferStatus::Transferring,
        };
        assert!(transfers.create(&transfer).is_ok());
        assert!(transfers
            .update_progress(transfer.id, 1024, TransferStatus::Completed)
            .is_ok());
        let transfers = transfers
            .list_by_host(host_id)
            .unwrap_or_else(|error| unreachable!("list transfers: {error}"));
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].status, TransferStatus::Completed);
        assert_eq!(transfers[0].transferred_bytes, 1024);

        let tunnel_repo = TunnelRepository::new(db.connection());
        let tunnel = Tunnel {
            id: TunnelId::new(),
            host_id,
            remote_host: "127.0.0.1".into(),
            remote_port: 3000,
            local_port: 54321,
        };
        assert!(tunnel_repo.create(&tunnel).is_ok());
        let tunnels = tunnel_repo
            .list_by_host(host_id)
            .unwrap_or_else(|error| unreachable!("list tunnels: {error}"));
        assert_eq!(tunnels.len(), 1);
        assert!(tunnel_repo.delete(tunnel.id).ok() == Some(true));

        // Snippets reference a real project row.
        let project_repo = ProjectRepository::new(db.connection());
        let project_id = ProjectId::new();
        project_repo
            .upsert(&Project {
                id: project_id,
                host_id,
                name: "snippets".into(),
                remote_cwd: "~/code".into(),
                preferred_runtime: RuntimeKind::Tmux,
            })
            .unwrap_or_else(|error| unreachable!("project seed failed: {error:?}"));
        let snippet_repo = SnippetRepository::new(db.connection());
        let snippet = Snippet {
            id: SnippetId::new(),
            name: "status".into(),
            text: "git status".into(),
            sort_order: 0,
        };
        assert!(snippet_repo.upsert(&snippet).is_ok());
        let snippets = snippet_repo
            .list_all()
            .unwrap_or_else(|error| unreachable!("list snippets: {error}"));
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "status");
        assert!(snippet_repo.delete(snippet.id).ok() == Some(true));
    }
}
