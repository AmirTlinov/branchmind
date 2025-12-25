#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
    RevisionMismatch { expected: i64, actual: i64 },
    UnknownId,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::RevisionMismatch { expected, actual } => {
                write!(f, "revision mismatch (expected={expected}, actual={actual})")
            }
            Self::UnknownId => write!(f, "unknown id"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Clone, Debug)]
pub struct PlanRow {
    pub id: String,
    pub revision: i64,
    pub title: String,
    pub contract: Option<String>,
    pub contract_json: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub id: String,
    pub revision: i64,
    pub parent_plan_id: String,
    pub title: String,
    pub description: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct ReasoningRefRow {
    pub branch: String,
    pub notes_doc: String,
    pub graph_doc: String,
    pub trace_doc: String,
}

#[derive(Clone, Debug)]
pub struct EventRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub event_type: String,
    pub payload_json: String,
}

impl EventRow {
    pub fn event_id(&self) -> String {
        format!("evt_{:016}", self.seq)
    }
}

#[derive(Debug)]
pub struct SqliteStore {
    storage_dir: PathBuf,
    conn: Connection,
}

impl SqliteStore {
    pub fn open(storage_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&storage_dir)?;
        let db_path = storage_dir.join("branchmind_rust.db");
        let conn = Connection::open(db_path)?;
        let store = Self { storage_dir, conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workspaces (
              workspace TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS counters (
              workspace TEXT NOT NULL,
              name TEXT NOT NULL,
              value INTEGER NOT NULL,
              PRIMARY KEY (workspace, name)
            );

            CREATE TABLE IF NOT EXISTS plans (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              revision INTEGER NOT NULL,
              title TEXT NOT NULL,
              contract TEXT,
              contract_json TEXT,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE TABLE IF NOT EXISTS tasks (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              revision INTEGER NOT NULL,
              parent_plan_id TEXT NOT NULL,
              title TEXT NOT NULL,
              description TEXT,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE TABLE IF NOT EXISTS events (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              task_id TEXT,
              path TEXT,
              type TEXT NOT NULL,
              payload_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS focus (
              workspace TEXT PRIMARY KEY,
              focus_id TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reasoning_refs (
              workspace TEXT NOT NULL,
              id TEXT NOT NULL,
              kind TEXT NOT NULL,
              branch TEXT NOT NULL,
              notes_doc TEXT NOT NULL,
              graph_doc TEXT NOT NULL,
              trace_doc TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, id)
            );

            CREATE INDEX IF NOT EXISTS idx_events_workspace_seq ON events(workspace, seq);
            "#,
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES (?1, ?2)",
            params!["schema_version", "v0"],
        )?;
        Ok(())
    }

    pub fn create(
        &mut self,
        workspace: &WorkspaceId,
        kind: TaskKind,
        title: String,
        parent_plan_id: Option<String>,
        description: Option<String>,
        contract: Option<String>,
        contract_json: Option<String>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(String, i64, EventRow), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let id = match kind {
            TaskKind::Plan => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "plan_seq")?;
                format!("PLAN-{:03}", seq)
            }
            TaskKind::Task => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "task_seq")?;
                format!("TASK-{:03}", seq)
            }
        };

        match kind {
            TaskKind::Plan => {
                tx.execute(
                    r#"
                    INSERT INTO plans(workspace,id,revision,title,contract,contract_json,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        title,
                        contract,
                        contract_json,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
            TaskKind::Task => {
                let parent_plan_id = parent_plan_id.ok_or(StoreError::InvalidInput("parent is required for kind=task"))?;
                tx.execute(
                    r#"
                    INSERT INTO tasks(workspace,id,revision,parent_plan_id,title,description,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        parent_plan_id,
                        title,
                        description,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
        }

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            match kind {
                TaskKind::Plan => Some(id.clone()),
                TaskKind::Task => Some(id.clone()),
            },
            None,
            &event_type,
            &event_payload_json,
        )?;

        tx.commit()?;
        Ok((id, 0i64, event))
    }

    pub fn edit_plan(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        title: Option<String>,
        contract: Option<Option<String>>,
        contract_json: Option<Option<String>>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        if title.is_none() && contract.is_none() && contract_json.is_none() {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, contract, contract_json
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((revision, current_title, current_contract, current_contract_json)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        let new_revision = revision + 1;
        let new_title = title.unwrap_or(current_title);
        let new_contract = contract.unwrap_or(current_contract);
        let new_contract_json = contract_json.unwrap_or(current_contract_json);

        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3, title = ?4, contract = ?5, contract_json = ?6, updated_at_ms = ?7
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                id,
                new_revision,
                new_title,
                new_contract,
                new_contract_json,
                now_ms
            ],
        )?;

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn edit_task(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        expected_revision: Option<i64>,
        title: Option<String>,
        description: Option<Option<String>>,
        event_type: String,
        event_payload_json: String,
    ) -> Result<(i64, EventRow), StoreError> {
        if title.is_none() && description.is_none() {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, description
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
            )
            .optional()?;

        let Some((revision, current_title, current_description)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(StoreError::RevisionMismatch {
                    expected,
                    actual: revision,
                });
            }
        }

        let new_revision = revision + 1;
        let new_title = title.unwrap_or(current_title);
        let new_description = description.unwrap_or(current_description);

        tx.execute(
            r#"
            UPDATE tasks
            SET revision = ?3, title = ?4, description = ?5, updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![workspace.as_str(), id, new_revision, new_title, new_description, now_ms],
        )?;

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(id.to_string()),
            None,
            &event_type,
            &event_payload_json,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn get_plan(&self, workspace: &WorkspaceId, id: &str) -> Result<Option<PlanRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, title, contract, contract_json, created_at_ms, updated_at_ms
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(PlanRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        title: row.get(2)?,
                        contract: row.get(3)?,
                        contract_json: row.get(4)?,
                        created_at_ms: row.get(5)?,
                        updated_at_ms: row.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn get_task(&self, workspace: &WorkspaceId, id: &str) -> Result<Option<TaskRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                r#"
                SELECT id, revision, parent_plan_id, title, description, created_at_ms, updated_at_ms
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(TaskRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        parent_plan_id: row.get(2)?,
                        title: row.get(3)?,
                        description: row.get(4)?,
                        created_at_ms: row.get(5)?,
                        updated_at_ms: row.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn focus_set(&mut self, workspace: &WorkspaceId, focus_id: &str) -> Result<(), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        tx.execute(
            r#"
            INSERT INTO focus(workspace, focus_id, updated_at_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace) DO UPDATE SET focus_id=excluded.focus_id, updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace.as_str(), focus_id, now_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn focus_get(&self, workspace: &WorkspaceId) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT focus_id FROM focus WHERE workspace = ?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn focus_clear(&mut self, workspace: &WorkspaceId) -> Result<bool, StoreError> {
        let tx = self.conn.transaction()?;
        let deleted = tx.execute("DELETE FROM focus WHERE workspace = ?1", params![workspace.as_str()])?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn ensure_reasoning_ref(
        &mut self,
        workspace: &WorkspaceId,
        id: &str,
        kind: TaskKind,
    ) -> Result<ReasoningRefRow, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let exists = match kind {
            TaskKind::Plan => tx
                .query_row(
                    "SELECT 1 FROM plans WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
            TaskKind::Task => tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                    params![workspace.as_str(), id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some(),
        };

        if !exists {
            return Err(StoreError::UnknownId);
        }

        if let Some(row) = tx
            .query_row(
                r#"
                SELECT branch, notes_doc, graph_doc, trace_doc
                FROM reasoning_refs
                WHERE workspace=?1 AND id=?2
                "#,
                params![workspace.as_str(), id],
                |row| {
                    Ok(ReasoningRefRow {
                        branch: row.get(0)?,
                        notes_doc: row.get(1)?,
                        graph_doc: row.get(2)?,
                        trace_doc: row.get(3)?,
                    })
                },
            )
            .optional()?
        {
            tx.commit()?;
            return Ok(row);
        }

        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let reference = ReasoningRef::for_entity(kind, id);
        tx.execute(
            r#"
            INSERT INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                workspace.as_str(),
                id,
                kind.as_str(),
                reference.branch,
                reference.notes_doc,
                reference.graph_doc,
                reference.trace_doc,
                now_ms
            ],
        )?;

        let row = ReasoningRefRow {
            branch: reference.branch,
            notes_doc: reference.notes_doc,
            graph_doc: reference.graph_doc,
            trace_doc: reference.trace_doc,
        };

        tx.commit()?;
        Ok(row)
    }

    pub fn list_plans(&self, workspace: &WorkspaceId, limit: usize, offset: usize) -> Result<Vec<PlanRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, title, contract, contract_json, created_at_ms, updated_at_ms
            FROM plans
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
            |row| {
                Ok(PlanRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    title: row.get(2)?,
                    contract: row.get(3)?,
                    contract_json: row.get(4)?,
                    created_at_ms: row.get(5)?,
                    updated_at_ms: row.get(6)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_tasks(&self, workspace: &WorkspaceId, limit: usize, offset: usize) -> Result<Vec<TaskRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, revision, parent_plan_id, title, description, created_at_ms, updated_at_ms
            FROM tasks
            WHERE workspace = ?1
            ORDER BY id ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), limit as i64, offset as i64],
            |row| {
                Ok(TaskRow {
                    id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_plan_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    created_at_ms: row.get(5)?,
                    updated_at_ms: row.get(6)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_events(
        &self,
        workspace: &WorkspaceId,
        since_event_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        let since_seq = match since_event_id {
            None => 0i64,
            Some(event_id) => parse_event_id(event_id).ok_or(StoreError::InvalidInput("since must be like evt_<16-digit-seq>"))?,
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, type, payload_json
            FROM events
            WHERE workspace = ?1 AND seq > ?2
            ORDER BY seq ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), since_seq, limit as i64], |row| {
            Ok(EventRow {
                seq: row.get(0)?,
                ts_ms: row.get(1)?,
                task_id: row.get(2)?,
                path: row.get(3)?,
                event_type: row.get(4)?,
                payload_json: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

}

fn now_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

fn ensure_workspace_tx(tx: &Transaction<'_>, workspace: &WorkspaceId, now_ms: i64) -> Result<(), StoreError> {
    tx.execute(
        "INSERT OR IGNORE INTO workspaces(workspace, created_at_ms) VALUES (?1, ?2)",
        params![workspace.as_str(), now_ms],
    )?;
    Ok(())
}

fn next_counter_tx(tx: &Transaction<'_>, workspace: &str, name: &str) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT value FROM counters WHERE workspace=?1 AND name=?2",
            params![workspace, name],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let next = current + 1;
    tx.execute(
        r#"
        INSERT INTO counters(workspace, name, value) VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace, name) DO UPDATE SET value=excluded.value
        "#,
        params![workspace, name, next],
    )?;
    Ok(next)
}

fn insert_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    ts_ms: i64,
    task_id: Option<String>,
    path: Option<String>,
    event_type: &str,
    payload_json: &str,
) -> Result<EventRow, StoreError> {
    let task_id_for_return = task_id.clone();
    let path_for_return = path.clone();
    tx.execute(
        r#"
        INSERT INTO events(workspace, ts_ms, task_id, path, type, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![workspace, ts_ms, task_id, path, event_type, payload_json],
    )?;
    let seq = tx.last_insert_rowid();
    Ok(EventRow {
        seq,
        ts_ms,
        task_id: task_id_for_return,
        path: path_for_return,
        event_type: event_type.to_string(),
        payload_json: payload_json.to_string(),
    })
}

fn parse_event_id(event_id: &str) -> Option<i64> {
    let digits = event_id.strip_prefix("evt_")?;
    digits.parse::<i64>().ok()
}
