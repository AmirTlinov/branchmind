#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_core::model::{ReasoningRef, TaskKind};
use bm_core::paths::StepPath;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(&'static str),
    RevisionMismatch { expected: i64, actual: i64 },
    UnknownId,
    UnknownBranch,
    BranchAlreadyExists,
    BranchCycle,
    BranchDepthExceeded,
    StepNotFound,
    CheckpointsNotConfirmed { criteria: bool, tests: bool },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io: {err}"),
            Self::Sql(err) => write!(f, "sqlite: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::RevisionMismatch { expected, actual } => {
                write!(
                    f,
                    "revision mismatch (expected={expected}, actual={actual})"
                )
            }
            Self::UnknownId => write!(f, "unknown id"),
            Self::UnknownBranch => write!(f, "unknown branch"),
            Self::BranchAlreadyExists => write!(f, "branch already exists"),
            Self::BranchCycle => write!(f, "branch base cycle"),
            Self::BranchDepthExceeded => write!(f, "branch base depth exceeded"),
            Self::StepNotFound => write!(f, "step not found"),
            Self::CheckpointsNotConfirmed { criteria, tests } => {
                write!(
                    f,
                    "checkpoints not confirmed (criteria={criteria}, tests={tests})"
                )
            }
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
pub struct BranchInfo {
    pub name: String,
    pub base_branch: Option<String>,
    pub base_seq: Option<i64>,
    pub created_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Notes,
    Trace,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Trace => "trace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocEntryKind {
    Note,
    Event,
}

impl DocEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Event => "event",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DocEntryRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub branch: String,
    pub doc: String,
    pub kind: DocEntryKind,
    pub title: Option<String>,
    pub format: Option<String>,
    pub meta_json: Option<String>,
    pub content: Option<String>,
    pub source_event_id: Option<String>,
    pub event_type: Option<String>,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub payload_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocSlice {
    pub entries: Vec<DocEntryRow>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct MergeNotesResult {
    pub merged: usize,
    pub skipped: usize,
    pub count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct StepRef {
    pub step_id: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct StepOpResult {
    pub task_revision: i64,
    pub step: StepRef,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct DecomposeResult {
    pub task_revision: i64,
    pub steps: Vec<StepRef>,
    pub event: EventRow,
}

#[derive(Clone, Debug)]
pub struct NewStep {
    pub title: String,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StepStatus {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub completed: bool,
}

#[derive(Clone, Debug)]
pub struct TaskStepSummary {
    pub total_steps: i64,
    pub completed_steps: i64,
    pub open_steps: i64,
    pub missing_criteria: i64,
    pub missing_tests: i64,
    pub first_open: Option<StepStatus>,
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

    pub fn workspace_init(&mut self, workspace: &WorkspaceId) -> Result<(), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        tx.commit()?;
        Ok(())
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

            CREATE TABLE IF NOT EXISTS documents (
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              kind TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, branch, doc)
            );

            CREATE TABLE IF NOT EXISTS doc_entries (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              branch TEXT NOT NULL,
              doc TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              kind TEXT NOT NULL,
              title TEXT,
              format TEXT,
              meta_json TEXT,
              content TEXT,
              source_event_id TEXT,
              event_type TEXT,
              task_id TEXT,
              path TEXT,
              payload_json TEXT
            );

            CREATE TABLE IF NOT EXISTS branches (
              workspace TEXT NOT NULL,
              name TEXT NOT NULL,
              base_branch TEXT NOT NULL,
              base_seq INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, name)
            );

            CREATE TABLE IF NOT EXISTS branch_checkout (
              workspace TEXT PRIMARY KEY,
              branch TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS steps (
              workspace TEXT NOT NULL,
              task_id TEXT NOT NULL,
              step_id TEXT NOT NULL,
              parent_step_id TEXT,
              ordinal INTEGER NOT NULL,
              title TEXT NOT NULL,
              completed INTEGER NOT NULL,
              criteria_confirmed INTEGER NOT NULL,
              tests_confirmed INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (workspace, step_id)
            );

            CREATE TABLE IF NOT EXISTS step_criteria (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_tests (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_blockers (
              workspace TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ordinal INTEGER NOT NULL,
              text TEXT NOT NULL,
              PRIMARY KEY (workspace, step_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS step_notes (
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              workspace TEXT NOT NULL,
              task_id TEXT NOT NULL,
              step_id TEXT NOT NULL,
              ts_ms INTEGER NOT NULL,
              note TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_workspace_seq ON events(workspace, seq);
            CREATE INDEX IF NOT EXISTS idx_doc_entries_lookup ON doc_entries(workspace, branch, doc, seq);
            CREATE INDEX IF NOT EXISTS idx_doc_entries_workspace_seq ON doc_entries(workspace, seq);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_doc_entries_event_dedup ON doc_entries(workspace, branch, doc, source_event_id) WHERE source_event_id IS NOT NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_root_unique ON steps(workspace, task_id, ordinal) WHERE parent_step_id IS NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_child_unique ON steps(workspace, task_id, parent_step_id, ordinal) WHERE parent_step_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_steps_lookup ON steps(workspace, task_id, parent_step_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_step_notes_step_seq ON step_notes(workspace, task_id, step_id, seq);
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
                let parent_plan_id = parent_plan_id
                    .ok_or(StoreError::InvalidInput("parent is required for kind=task"))?;
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

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, &id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
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

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Plan, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
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
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
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
            params![
                workspace.as_str(),
                id,
                new_revision,
                new_title,
                new_description,
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

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn get_plan(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<PlanRow>, StoreError> {
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

    pub fn get_task(
        &self,
        workspace: &WorkspaceId,
        id: &str,
    ) -> Result<Option<TaskRow>, StoreError> {
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
        let deleted = tx.execute(
            "DELETE FROM focus WHERE workspace = ?1",
            params![workspace.as_str()],
        )?;
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

    pub fn doc_append_note(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        title: Option<String>,
        format: Option<String>,
        meta_json: Option<String>,
        content: String,
    ) -> Result<DocEntryRow, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if content.trim().is_empty() {
            return Err(StoreError::InvalidInput("content must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;

        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                workspace.as_str(),
                branch,
                doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                title.as_deref(),
                format.as_deref(),
                meta_json.as_deref(),
                &content
            ],
        )?;
        let seq = tx.last_insert_rowid();
        touch_document_tx(&tx, workspace.as_str(), branch, doc, now_ms)?;

        tx.commit()?;
        Ok(DocEntryRow {
            seq,
            ts_ms: now_ms,
            branch: branch.to_string(),
            doc: doc.to_string(),
            kind: DocEntryKind::Note,
            title,
            format,
            meta_json,
            content: Some(content),
            source_event_id: None,
            event_type: None,
            task_id: None,
            path: None,
            payload_json: None,
        })
    }

    pub fn doc_show_tail(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        let mut entries_desc = Vec::new();
        {
            let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

            let mut sql = String::from(
                "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
                 FROM doc_entries \
                 WHERE workspace=? AND doc=? AND seq < ? AND (",
            );
            let mut params: Vec<SqlValue> = Vec::new();
            params.push(SqlValue::Text(workspace.as_str().to_string()));
            params.push(SqlValue::Text(doc.to_string()));
            params.push(SqlValue::Integer(before_seq));

            for (index, source) in sources.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("(branch=?");
                params.push(SqlValue::Text(source.branch.clone()));
                if let Some(cutoff) = source.cutoff_seq {
                    sql.push_str(" AND seq <= ?");
                    params.push(SqlValue::Integer(cutoff));
                }
                sql.push(')');
            }

            sql.push_str(") ORDER BY seq DESC LIMIT ?");
            params.push(SqlValue::Integer(limit + 1));

            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params))?;

            while let Some(row) = rows.next()? {
                let kind_str: String = row.get(3)?;
                let kind = match kind_str.as_str() {
                    "note" => DocEntryKind::Note,
                    "event" => DocEntryKind::Event,
                    _ => DocEntryKind::Event,
                };
                entries_desc.push(DocEntryRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    branch: row.get(2)?,
                    doc: doc.to_string(),
                    kind,
                    title: row.get(4)?,
                    format: row.get(5)?,
                    meta_json: row.get(6)?,
                    content: row.get(7)?,
                    source_event_id: row.get(8)?,
                    event_type: row.get(9)?,
                    task_id: row.get(10)?,
                    path: row.get(11)?,
                    payload_json: row.get(12)?,
                });
            }
        }

        let has_more = entries_desc.len() as i64 > limit;
        if has_more {
            entries_desc.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            entries_desc.last().map(|e| e.seq)
        } else {
            None
        };

        entries_desc.reverse();
        tx.commit()?;

        Ok(DocSlice {
            entries: entries_desc,
            next_cursor,
            has_more,
        })
    }

    pub fn doc_diff_tail(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        to_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if to_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("to_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), to_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        let slice = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            from_branch,
            to_branch,
            doc,
            before_seq,
            limit,
        )?;
        tx.commit()?;
        Ok(slice)
    }

    pub fn doc_merge_notes(
        &mut self,
        workspace: &WorkspaceId,
        from_branch: &str,
        into_branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
        dry_run: bool,
    ) -> Result<MergeNotesResult, StoreError> {
        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), from_branch)?
            || !branch_exists_tx(&tx, workspace.as_str(), into_branch)?
        {
            return Err(StoreError::UnknownBranch);
        }

        if !dry_run {
            ensure_workspace_tx(&tx, workspace, now_ms)?;
            ensure_document_tx(
                &tx,
                workspace.as_str(),
                into_branch,
                doc,
                DocumentKind::Notes.as_str(),
                now_ms,
            )?;
        }

        // Merge candidates are entries present in sourceView(from_branch) but not in destView(into_branch).
        let diff = doc_diff_tail_tx(
            &tx,
            workspace.as_str(),
            into_branch,
            from_branch,
            doc,
            before_seq,
            limit,
        )?;

        let mut merged = 0usize;
        let mut skipped = 0usize;
        let mut touched = false;

        for entry in diff.entries.iter() {
            if entry.kind != DocEntryKind::Note {
                skipped += 1;
                continue;
            }

            let Some(content) = entry.content.as_deref() else {
                skipped += 1;
                continue;
            };

            let merge_key = format!("merge:{from_branch}:{}", entry.seq);
            if dry_run {
                let exists = tx
                    .query_row(
                        "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4 LIMIT 1",
                        params![workspace.as_str(), into_branch, doc, &merge_key],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if exists {
                    skipped += 1;
                } else {
                    merged += 1;
                }
                continue;
            }

            let meta_json = merge_meta_json(
                entry.meta_json.as_deref(),
                from_branch,
                entry.seq,
                entry.ts_ms,
            );

            let inserted = tx.execute(
                r#"
                INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, title, format, meta_json, content, source_event_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    workspace.as_str(),
                    into_branch,
                    doc,
                    now_ms,
                    DocEntryKind::Note.as_str(),
                    entry.title.as_deref(),
                    entry.format.as_deref(),
                    &meta_json,
                    content,
                    &merge_key
                ],
            )?;

            if inserted > 0 {
                merged += 1;
                touched = true;
            } else {
                skipped += 1;
            }
        }

        if touched {
            touch_document_tx(&tx, workspace.as_str(), into_branch, doc, now_ms)?;
        }

        tx.commit()?;
        Ok(MergeNotesResult {
            merged,
            skipped,
            count: diff.entries.len(),
            next_cursor: diff.next_cursor,
            has_more: diff.has_more,
        })
    }

    pub fn doc_ingest_task_event(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        event: &EventRow,
    ) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, event.ts_ms)?;
        let inserted = ingest_task_event_tx(&tx, workspace.as_str(), branch, doc, event)?;
        tx.commit()?;
        Ok(inserted)
    }

    pub fn branch_checkout_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn branch_checkout_set(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<(Option<String>, String), StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let previous = tx
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        tx.execute(
            r#"
            INSERT INTO branch_checkout(workspace, branch, updated_at_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace) DO UPDATE SET branch=excluded.branch, updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace.as_str(), branch, now_ms],
        )?;

        tx.commit()?;
        Ok((previous, branch.to_string()))
    }

    pub fn branch_create(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
        from: Option<&str>,
    ) -> Result<BranchInfo, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if branch_exists_tx(&tx, workspace.as_str(), name)? {
            return Err(StoreError::BranchAlreadyExists);
        }

        let base_branch = match from {
            Some(v) if !v.trim().is_empty() => v.to_string(),
            Some(_) => return Err(StoreError::InvalidInput("from must not be empty")),
            None => tx
                .query_row(
                    "SELECT branch FROM branch_checkout WHERE workspace=?1",
                    params![workspace.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(StoreError::InvalidInput(
                    "from is required when no checkout branch is set",
                ))?,
        };

        if !branch_exists_tx(&tx, workspace.as_str(), &base_branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let base_seq = doc_entries_head_seq_tx(&tx, workspace.as_str())?.unwrap_or(0);

        tx.execute(
            r#"
            INSERT INTO branches(workspace, name, base_branch, base_seq, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                workspace.as_str(),
                name,
                base_branch.as_str(),
                base_seq,
                now_ms
            ],
        )?;

        tx.commit()?;
        Ok(BranchInfo {
            name: name.to_string(),
            base_branch: Some(base_branch),
            base_seq: Some(base_seq),
            created_at_ms: Some(now_ms),
        })
    }

    pub fn branch_list(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
    ) -> Result<Vec<BranchInfo>, StoreError> {
        use std::collections::HashMap;

        let limit = limit.clamp(1, 500);
        let mut map: HashMap<String, BranchInfo> = HashMap::new();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT name, base_branch, base_seq, created_at_ms
            FROM branches
            WHERE workspace=?1
            ORDER BY name ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str()], |row| {
            Ok(BranchInfo {
                name: row.get::<_, String>(0)?,
                base_branch: Some(row.get::<_, String>(1)?),
                base_seq: Some(row.get::<_, i64>(2)?),
                created_at_ms: Some(row.get::<_, i64>(3)?),
            })
        })?;
        for row in rows {
            let info = row?;
            map.insert(info.name.clone(), info);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM reasoning_refs WHERE workspace=?1")?;
        let refs = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in refs {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM doc_entries WHERE workspace=?1")?;
        let entries = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in entries {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut names = map.keys().cloned().collect::<Vec<_>>();
        names.sort();
        let mut out = Vec::new();
        for name in names.into_iter().take(limit) {
            if let Some(info) = map.remove(&name) {
                out.push(info);
            }
        }
        Ok(out)
    }

    pub fn branch_exists(&self, workspace: &WorkspaceId, branch: &str) -> Result<bool, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        if self
            .conn
            .query_row(
                "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
                params![workspace.as_str(), branch],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(true);
        }

        Ok(false)
    }

    pub fn steps_decompose(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        parent_path: Option<&StepPath>,
        steps: Vec<NewStep>,
    ) -> Result<DecomposeResult, StoreError> {
        if steps.is_empty() {
            return Err(StoreError::InvalidInput("steps must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;

        let parent_step_id = match parent_path {
            None => None,
            Some(path) => Some(resolve_step_id_tx(&tx, workspace.as_str(), task_id, path)?),
        };

        let max_ordinal: Option<i64> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL",
                    params![workspace.as_str(), task_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                    params![workspace.as_str(), task_id, parent_step_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
        };

        let mut next_ordinal = max_ordinal.unwrap_or(-1) + 1;
        let mut created_steps = Vec::with_capacity(steps.len());

        for step in steps {
            let seq = next_counter_tx(&tx, workspace.as_str(), "step_seq")?;
            let step_id = format!("STEP-{seq:08X}");
            let ordinal = next_ordinal;
            next_ordinal += 1;

            tx.execute(
                r#"
                INSERT INTO steps(workspace,task_id,step_id,parent_step_id,ordinal,title,completed,criteria_confirmed,tests_confirmed,created_at_ms,updated_at_ms)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    parent_step_id,
                    ordinal,
                    step.title,
                    0i64,
                    0i64,
                    0i64,
                    now_ms,
                    now_ms
                ],
            )?;

            for (i, text) in step.success_criteria.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }

            let path = match parent_path {
                None => StepPath::root(ordinal as usize).to_string(),
                Some(parent) => parent.child(ordinal as usize).to_string(),
            };
            created_steps.push(StepRef { step_id, path });
        }

        let parent_path_str = parent_path.map(|p| p.to_string());
        let event_payload_json =
            build_steps_added_payload(task_id, parent_path_str.as_deref(), &created_steps);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            parent_path_str,
            "steps_added",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok(DecomposeResult {
            task_revision,
            steps: created_steps,
            event,
        })
    }

    pub fn step_define(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        title: Option<String>,
        success_criteria: Option<Vec<String>>,
        tests: Option<Vec<String>>,
        blockers: Option<Vec<String>>,
    ) -> Result<StepOpResult, StoreError> {
        if title.is_none() && success_criteria.is_none() && tests.is_none() && blockers.is_none() {
            return Err(StoreError::InvalidInput("no fields to define"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let mut fields = Vec::new();

        if let Some(title) = title {
            tx.execute(
                "UPDATE steps SET title=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, title, now_ms],
            )?;
            fields.push("title");
        }

        if let Some(items) = success_criteria {
            tx.execute(
                "DELETE FROM step_criteria WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET criteria_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("success_criteria");
        }

        if let Some(items) = tests {
            tx.execute(
                "DELETE FROM step_tests WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_tests(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            tx.execute(
                "UPDATE steps SET tests_confirmed=0, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, now_ms],
            )?;
            fields.push("tests");
        }

        if let Some(items) = blockers {
            tx.execute(
                "DELETE FROM step_blockers WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
            for (i, text) in items.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_blockers(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }
            fields.push("blockers");
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_defined_payload(task_id, &step_ref, &fields);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_defined",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_note(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        note: String,
    ) -> Result<StepOpResult, StoreError> {
        if note.trim().is_empty() {
            return Err(StoreError::InvalidInput("note must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        tx.execute(
            "INSERT INTO step_notes(workspace, task_id, step_id, ts_ms, note) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workspace.as_str(), task_id, step_id, now_ms, &note],
        )?;
        let note_seq = tx.last_insert_rowid();

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_noted_payload(task_id, &step_ref, note_seq);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_noted",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        // Mirror the human-authored note content into the reasoning notes document (single organism invariant).
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            DocumentKind::Notes.as_str(),
            now_ms,
        )?;
        let meta_json =
            build_step_noted_mirror_meta_json(task_id, &step_ref, note_seq, &event.event_id());
        tx.execute(
            r#"
            INSERT INTO doc_entries(workspace, branch, doc, ts_ms, kind, meta_json, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.notes_doc,
                now_ms,
                DocEntryKind::Note.as_str(),
                meta_json,
                &note
            ],
        )?;
        touch_document_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.notes_doc,
            now_ms,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_verify(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
        criteria_confirmed: Option<bool>,
        tests_confirmed: Option<bool>,
    ) -> Result<StepOpResult, StoreError> {
        if criteria_confirmed.is_none() && tests_confirmed.is_none() {
            return Err(StoreError::InvalidInput("no checkpoints to verify"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;
        if criteria_confirmed.is_some() && tests_confirmed.is_some() {
            tx.execute(
                "UPDATE steps SET criteria_confirmed=?4, tests_confirmed=?5, updated_at_ms=?6 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    if criteria_confirmed.unwrap() { 1i64 } else { 0i64 },
                    if tests_confirmed.unwrap() { 1i64 } else { 0i64 },
                    now_ms
                ],
            )?;
        } else if let Some(v) = criteria_confirmed {
            tx.execute(
                "UPDATE steps SET criteria_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        } else if let Some(v) = tests_confirmed {
            tx.execute(
                "UPDATE steps SET tests_confirmed=?4, updated_at_ms=?5 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id, if v { 1i64 } else { 0i64 }, now_ms],
            )?;
        }

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json =
            build_step_verified_payload(task_id, &step_ref, criteria_confirmed, tests_confirmed);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_verified",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn step_done(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        step_id: Option<&str>,
        path: Option<&StepPath>,
    ) -> Result<StepOpResult, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;
        let (step_id, path) =
            resolve_step_selector_tx(&tx, workspace.as_str(), task_id, step_id, path)?;

        let row = tx
            .query_row(
                "SELECT completed, criteria_confirmed, tests_confirmed FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace.as_str(), task_id, step_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()?;

        let Some((completed, criteria_confirmed, tests_confirmed)) = row else {
            return Err(StoreError::StepNotFound);
        };

        if completed != 0 {
            return Err(StoreError::InvalidInput("step already completed"));
        }

        if criteria_confirmed == 0 || tests_confirmed == 0 {
            return Err(StoreError::CheckpointsNotConfirmed {
                criteria: criteria_confirmed == 0,
                tests: tests_confirmed == 0,
            });
        }

        tx.execute(
            "UPDATE steps SET completed=1, updated_at_ms=?4 WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
            params![workspace.as_str(), task_id, step_id, now_ms],
        )?;

        let step_ref = StepRef {
            step_id: step_id.clone(),
            path: path.clone(),
        };
        let event_payload_json = build_step_done_payload(task_id, &step_ref);
        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            Some(task_id.to_string()),
            Some(path.clone()),
            "step_done",
            &event_payload_json,
        )?;

        let reasoning_ref =
            ensure_reasoning_ref_tx(&tx, workspace, task_id, TaskKind::Task, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        tx.commit()?;
        Ok(StepOpResult {
            task_revision,
            step: step_ref,
            event,
        })
    }

    pub fn task_steps_summary(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
    ) -> Result<TaskStepSummary, StoreError> {
        let tx = self.conn.transaction()?;

        let exists = tx
            .query_row(
                "SELECT 1 FROM tasks WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), task_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(StoreError::UnknownId);
        }

        let total_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let completed_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=1",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let open_steps: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_criteria: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND criteria_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;
        let missing_tests: i64 = tx.query_row(
            "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0 AND tests_confirmed=0",
            params![workspace.as_str(), task_id],
            |row| row.get(0),
        )?;

        let first_open = tx
            .query_row(
                r#"
                SELECT step_id, title, completed, criteria_confirmed, tests_confirmed
                FROM steps
                WHERE workspace=?1 AND task_id=?2 AND completed=0
                ORDER BY created_at_ms ASC
                LIMIT 1
                "#,
                params![workspace.as_str(), task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
            .map(|(step_id, title, completed, criteria, tests)| {
                let path = step_path_for_step_id_tx(&tx, workspace.as_str(), task_id, &step_id)
                    .unwrap_or_else(|_| "s:?".to_string());
                StepStatus {
                    step_id,
                    path,
                    title,
                    completed: completed != 0,
                    criteria_confirmed: criteria != 0,
                    tests_confirmed: tests != 0,
                }
            });

        tx.commit()?;
        Ok(TaskStepSummary {
            total_steps,
            completed_steps,
            open_steps,
            missing_criteria,
            missing_tests,
            first_open,
        })
    }

    pub fn task_open_blockers(
        &self,
        workspace: &WorkspaceId,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT b.text
            FROM step_blockers b
            JOIN steps s
              ON s.workspace = b.workspace AND s.step_id = b.step_id
            WHERE s.workspace = ?1 AND s.task_id = ?2 AND s.completed = 0
            ORDER BY s.created_at_ms ASC, b.ordinal ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), task_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_plans(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<PlanRow>, StoreError> {
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

    pub fn list_tasks(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TaskRow>, StoreError> {
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
            Some(event_id) => parse_event_id(event_id).ok_or(StoreError::InvalidInput(
                "since must be like evt_<16-digit-seq>",
            ))?,
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
        let rows = stmt.query_map(
            params![workspace.as_str(), since_seq, limit as i64],
            |row| {
                Ok(EventRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    task_id: row.get(2)?,
                    path: row.get(3)?,
                    event_type: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn workspace_exists(&self, workspace: &WorkspaceId) -> Result<bool, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM workspaces WHERE workspace=?1",
                params![workspace.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn workspace_last_event_head(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<(i64, i64)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, ts_ms FROM events WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
                params![workspace.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?)
    }

    pub fn workspace_last_doc_entry_head(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<(i64, i64, String, String, String)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, ts_ms, branch, doc, kind FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
                params![workspace.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?)
    }
}

fn now_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

fn ensure_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    kind: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO documents(workspace, branch, doc, kind, created_at_ms, updated_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(workspace, branch, doc) DO NOTHING
        "#,
        params![workspace, branch, doc, kind, now_ms, now_ms],
    )?;
    Ok(())
}

fn touch_document_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE documents SET updated_at_ms=?4 WHERE workspace=?1 AND branch=?2 AND doc=?3",
        params![workspace, branch, doc, now_ms],
    )?;
    Ok(())
}

fn ensure_reasoning_ref_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    id: &str,
    kind: TaskKind,
    now_ms: i64,
) -> Result<ReasoningRefRow, StoreError> {
    let reference = ReasoningRef::for_entity(kind, id);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO reasoning_refs(workspace, id, kind, branch, notes_doc, graph_doc, trace_doc, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace.as_str(),
            id,
            kind.as_str(),
            &reference.branch,
            &reference.notes_doc,
            &reference.graph_doc,
            &reference.trace_doc,
            now_ms
        ],
    )?;
    Ok(ReasoningRefRow {
        branch: reference.branch,
        notes_doc: reference.notes_doc,
        graph_doc: reference.graph_doc,
        trace_doc: reference.trace_doc,
    })
}

fn ingest_task_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    event: &EventRow,
) -> Result<bool, StoreError> {
    ensure_document_tx(
        tx,
        workspace,
        branch,
        doc,
        DocumentKind::Trace.as_str(),
        event.ts_ms,
    )?;

    let event_id = event.event_id();
    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, source_event_id, event_type, task_id, path, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            workspace,
            branch,
            doc,
            event.ts_ms,
            DocEntryKind::Event.as_str(),
            event_id,
            &event.event_type,
            event.task_id.as_deref(),
            event.path.as_deref(),
            &event.payload_json
        ],
    )?;

    if inserted > 0 {
        touch_document_tx(tx, workspace, branch, doc, event.ts_ms)?;
    }

    Ok(inserted > 0)
}

fn doc_entries_head_seq_tx(
    tx: &Transaction<'_>,
    workspace: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT seq FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
            params![workspace],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn branch_exists_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<bool, StoreError> {
    if tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM reasoning_refs WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    if tx
        .query_row(
            "SELECT 1 FROM doc_entries WHERE workspace=?1 AND branch=?2 LIMIT 1",
            params![workspace, branch],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(true);
    }

    Ok(false)
}

#[derive(Clone, Debug)]
struct BranchSource {
    branch: String,
    cutoff_seq: Option<i64>,
}

fn branch_sources_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
) -> Result<Vec<BranchSource>, StoreError> {
    use std::collections::HashSet;

    const MAX_DEPTH: usize = 32;

    let mut sources = Vec::new();
    sources.push(BranchSource {
        branch: branch.to_string(),
        cutoff_seq: None,
    });

    let mut seen = HashSet::new();
    seen.insert(branch.to_string());

    let mut current = branch.to_string();
    let mut inherited_cutoff: Option<i64> = None;

    for depth in 0..MAX_DEPTH {
        let row = tx
            .query_row(
                "SELECT base_branch, base_seq FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace, current],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((base_branch, base_seq)) = row else {
            break;
        };

        if seen.contains(&base_branch) {
            return Err(StoreError::BranchCycle);
        }

        let effective = match inherited_cutoff {
            None => base_seq,
            Some(prev) => std::cmp::min(prev, base_seq),
        };

        sources.push(BranchSource {
            branch: base_branch.clone(),
            cutoff_seq: Some(effective),
        });

        seen.insert(base_branch.clone());
        current = base_branch;
        inherited_cutoff = Some(effective);

        if depth == MAX_DEPTH - 1 {
            return Err(StoreError::BranchDepthExceeded);
        }
    }

    Ok(sources)
}

fn doc_diff_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    to_branch: &str,
    doc: &str,
    before_seq: i64,
    limit: i64,
) -> Result<DocSlice, StoreError> {
    let from_sources = branch_sources_tx(tx, workspace, from_branch)?;
    let to_sources = branch_sources_tx(tx, workspace, to_branch)?;

    let mut sql = String::from(
        "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
         FROM doc_entries \
         WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));

    append_sources_clause(&mut sql, &mut params, &to_sources);
    sql.push_str(" AND NOT ");
    append_sources_clause(&mut sql, &mut params, &from_sources);
    sql.push_str(" ORDER BY seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit + 1));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;

    let mut entries_desc = Vec::new();
    while let Some(row) = rows.next()? {
        let kind_str: String = row.get(3)?;
        let kind = match kind_str.as_str() {
            "note" => DocEntryKind::Note,
            "event" => DocEntryKind::Event,
            _ => DocEntryKind::Event,
        };
        entries_desc.push(DocEntryRow {
            seq: row.get(0)?,
            ts_ms: row.get(1)?,
            branch: row.get(2)?,
            doc: doc.to_string(),
            kind,
            title: row.get(4)?,
            format: row.get(5)?,
            meta_json: row.get(6)?,
            content: row.get(7)?,
            source_event_id: row.get(8)?,
            event_type: row.get(9)?,
            task_id: row.get(10)?,
            path: row.get(11)?,
            payload_json: row.get(12)?,
        });
    }

    let has_more = entries_desc.len() as i64 > limit;
    if has_more {
        entries_desc.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        entries_desc.last().map(|e| e.seq)
    } else {
        None
    };

    entries_desc.reverse();

    Ok(DocSlice {
        entries: entries_desc,
        next_cursor,
        has_more,
    })
}

fn append_sources_clause(sql: &mut String, params: &mut Vec<SqlValue>, sources: &[BranchSource]) {
    sql.push('(');
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(branch=?");
        params.push(SqlValue::Text(source.branch.clone()));
        if let Some(cutoff) = source.cutoff_seq {
            sql.push_str(" AND seq <= ?");
            params.push(SqlValue::Integer(cutoff));
        }
        sql.push(')');
    }
    sql.push(')');
}

fn merge_meta_json(
    existing_meta_json: Option<&str>,
    from_branch: &str,
    from_seq: i64,
    from_ts_ms: i64,
) -> String {
    let payload = format!(
        r#"{{"from":"{}","from_seq":{},"from_ts_ms":{}}}"#,
        json_escape(from_branch),
        from_seq,
        from_ts_ms
    );

    let Some(raw) = existing_meta_json else {
        return format!(r#"{{"_merge":{payload}}}"#);
    };

    let trimmed = raw.trim();
    if looks_like_json_object(trimmed) {
        if trimmed == "{}" {
            return format!(r#"{{"_merge":{payload}}}"#);
        }

        if trimmed.contains("\"_merge\"") {
            return format!(r#"{{"_merge":{payload},"_meta":{trimmed}}}"#);
        }

        let mut out = trimmed.to_string();
        out.pop(); // remove trailing '}'
        if !out.trim_end().ends_with('{') {
            out.push(',');
        }
        out.push_str(&format!(r#""_merge":{payload}}}"#));
        return out;
    }

    format!(
        r#"{{"_merge":{payload},"_meta_raw":"{}"}}"#,
        json_escape(trimmed)
    )
}

fn looks_like_json_object(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}')
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn ensure_workspace_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT OR IGNORE INTO workspaces(workspace, created_at_ms) VALUES (?1, ?2)",
        params![workspace.as_str(), now_ms],
    )?;
    Ok(())
}

fn bump_task_revision_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    expected_revision: Option<i64>,
    now_ms: i64,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM tasks WHERE workspace=?1 AND id=?2",
            params![workspace, task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::UnknownId)?;

    if let Some(expected) = expected_revision {
        if expected != current {
            return Err(StoreError::RevisionMismatch {
                expected,
                actual: current,
            });
        }
    }

    let next = current + 1;
    tx.execute(
        "UPDATE tasks SET revision=?3, updated_at_ms=?4 WHERE workspace=?1 AND id=?2",
        params![workspace, task_id, next, now_ms],
    )?;
    Ok(next)
}

fn resolve_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    path: &StepPath,
) -> Result<String, StoreError> {
    let mut parent_step_id: Option<String> = None;
    for ordinal in path.indices() {
        let step_id: Option<String> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL AND ordinal=?3",
                    params![workspace, task_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT step_id FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3 AND ordinal=?4",
                    params![workspace, task_id, parent_step_id, *ordinal as i64],
                    |row| row.get(0),
                )
                .optional()?,
        };

        let Some(step_id) = step_id else {
            return Err(StoreError::StepNotFound);
        };
        parent_step_id = Some(step_id);
    }
    parent_step_id.ok_or(StoreError::StepNotFound)
}

fn step_path_for_step_id_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: &str,
) -> Result<String, StoreError> {
    let mut ordinals = Vec::new();
    let mut current = step_id.to_string();

    for _ in 0..128 {
        let row = tx
            .query_row(
                "SELECT parent_step_id, ordinal FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                params![workspace, task_id, current],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((parent, ordinal)) = row else {
            return Err(StoreError::StepNotFound);
        };
        ordinals.push(ordinal as usize);
        match parent {
            None => break,
            Some(parent_id) => current = parent_id,
        }
    }

    ordinals.reverse();
    Ok(ordinals
        .into_iter()
        .map(|i| format!("s:{i}"))
        .collect::<Vec<_>>()
        .join("."))
}

fn resolve_step_selector_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    task_id: &str,
    step_id: Option<&str>,
    path: Option<&StepPath>,
) -> Result<(String, String), StoreError> {
    match (step_id, path) {
        (Some(step_id), _) => {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM steps WHERE workspace=?1 AND task_id=?2 AND step_id=?3",
                    params![workspace, task_id, step_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::StepNotFound);
            }
            Ok((
                step_id.to_string(),
                step_path_for_step_id_tx(tx, workspace, task_id, step_id)?,
            ))
        }
        (None, Some(path)) => {
            let step_id = resolve_step_id_tx(tx, workspace, task_id, path)?;
            Ok((step_id, path.to_string()))
        }
        (None, None) => Err(StoreError::InvalidInput("step selector is required")),
    }
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

fn build_steps_added_payload(
    task_id: &str,
    parent_path: Option<&str>,
    steps: &[StepRef],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"parent_path\":");
    match parent_path {
        None => out.push_str("null"),
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
    }
    out.push_str(",\"steps\":[");
    for (i, step) in steps.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push_str("{\"step_id\":\"");
        out.push_str(&step.step_id);
        out.push_str("\",\"path\":\"");
        out.push_str(&step.path);
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

fn build_step_defined_payload(task_id: &str, step: &StepRef, fields: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push_str("\",\"fields\":[");
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(field);
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn build_step_noted_payload(task_id: &str, step: &StepRef, note_seq: i64) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq}}}",
        step.step_id, step.path
    )
}

fn build_step_noted_mirror_meta_json(
    task_id: &str,
    step: &StepRef,
    note_seq: i64,
    event_id: &str,
) -> String {
    format!(
        "{{\"source\":\"tasks_note\",\"task_id\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq},\"event_id\":\"{event_id}\"}}",
        step.step_id, step.path
    )
}

fn build_step_verified_payload(
    task_id: &str,
    step: &StepRef,
    criteria_confirmed: Option<bool>,
    tests_confirmed: Option<bool>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push('"');
    if let Some(v) = criteria_confirmed {
        out.push_str(",\"criteria_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = tests_confirmed {
        out.push_str(",\"tests_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    out.push('}');
    out
}

fn build_step_done_payload(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        step.step_id, step.path
    )
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
