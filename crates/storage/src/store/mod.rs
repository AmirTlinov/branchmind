#![forbid(unsafe_code)]
//! Storage implementation (split-friendly module root).

mod anchor_aliases;
mod anchor_bindings;
mod anchor_links;
mod anchors;
mod anchors_lint;
mod anchors_merge;
mod branches;
mod docs;
mod error;
mod focus;
mod graph;
mod job_bus;
mod jobs;
mod ops_history;
mod portal_cursors;
mod reasoning_ref;
mod requests;
mod runners;
mod slices;
mod steps;
mod support;
mod tasks;
mod think;
mod types;
mod v3;
mod vcs;

use bm_core::ids::WorkspaceId;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_BRANCH: &str = "main";
const V3_SCHEMA_VERSION: i64 = 3;

pub use error::StoreError;
pub use requests::*;
pub use types::*;

use support::*;

fn is_missing_table(err: &rusqlite::Error, table: &str) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => {
            msg.contains("no such table") && msg.contains(table)
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
enum OpsHistoryTarget {
    Task { title: Option<String> },
    Step { step: StepRef },
    TaskNode,
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
        conn.busy_timeout(Duration::from_secs(5))?;
        let preflight = v3_preflight_gate(&conn)?;
        let store = Self { storage_dir, conn };
        store.migrate()?;
        store.ensure_v3_schema(preflight)?;
        Ok(store)
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub fn default_branch_name(&self) -> &'static str {
        DEFAULT_BRANCH
    }

    pub fn next_card_id(&mut self, workspace: &WorkspaceId) -> Result<String, StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        let seq = next_counter_tx(&tx, workspace.as_str(), "card_seq")?;
        tx.commit()?;
        Ok(format!("CARD-{seq}"))
    }

    pub fn workspace_init(&mut self, workspace: &WorkspaceId) -> Result<(), StoreError> {
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        let _ = bootstrap_default_branch_tx(&tx, workspace.as_str(), now_ms)?;
        let _ = ensure_checkout_branch_tx(&tx, workspace.as_str(), DEFAULT_BRANCH, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    fn migrate(&self) -> Result<(), StoreError> {
        migrate_sqlite_schema(&self.conn)
    }

    fn ensure_v3_schema(&self, preflight: V3Preflight) -> Result<(), StoreError> {
        if matches!(preflight, V3Preflight::Empty) {
            install_v3_schema_extensions(&self.conn)?;
        }
        Ok(())
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

    pub fn list_workspaces(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkspaceRow>, StoreError> {
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset_i64 = i64::try_from(offset).unwrap_or(0);
        let mut stmt = self.conn.prepare(
            "SELECT workspace, created_at_ms, project_guard \
             FROM workspaces \
             ORDER BY created_at_ms DESC, workspace ASC \
             LIMIT ?1 OFFSET ?2",
        )?;
        let mut rows = stmt.query(params![limit_i64, offset_i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(WorkspaceRow {
                workspace: row.get(0)?,
                created_at_ms: row.get(1)?,
                project_guard: row.get(2)?,
            });
        }
        Ok(out)
    }

    /// Resolve a workspace id bound to a canonical filesystem path.
    ///
    /// Notes:
    /// - Intended for DX: callers may pass a repo path instead of a workspace id.
    /// - `path` should be canonical absolute path (caller can normalize; we treat it verbatim).
    pub fn workspace_path_resolve(
        &mut self,
        path: &str,
    ) -> Result<Option<WorkspaceId>, StoreError> {
        let path = path.trim();
        if path.is_empty() {
            return Ok(None);
        }

        let now_ms = now_ms();
        let stored = match self.conn.query_row(
            "SELECT workspace FROM workspace_paths WHERE path=?1",
            params![path],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(err) if is_missing_table(&err, "workspace_paths") => None,
            Err(err) => return Err(err.into()),
        };

        let Some(stored) = stored else {
            return Ok(None);
        };

        // Best-effort refresh last_used_at_ms (ignore missing-table drift for read-only consumers).
        let _ = self.conn.execute(
            "UPDATE workspace_paths SET last_used_at_ms=?1 WHERE path=?2",
            params![now_ms, path],
        );

        match WorkspaceId::try_new(stored) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    /// Bind a canonical filesystem path to a workspace id (idempotent).
    pub fn workspace_path_bind(
        &mut self,
        workspace: &WorkspaceId,
        path: &str,
    ) -> Result<(), StoreError> {
        let path = path.trim();
        if path.is_empty() {
            return Err(StoreError::InvalidInput("workspace path must not be empty"));
        }
        let now_ms = now_ms();

        let existing = match self.conn.query_row(
            "SELECT workspace FROM workspace_paths WHERE path=?1",
            params![path],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(err) if is_missing_table(&err, "workspace_paths") => None,
            Err(err) => return Err(err.into()),
        };

        if let Some(existing) = existing {
            if existing == workspace.as_str() {
                let _ = self.conn.execute(
                    "UPDATE workspace_paths SET last_used_at_ms=?1 WHERE path=?2",
                    params![now_ms, path],
                );
                return Ok(());
            }
            // Safe-by-default: do not silently rebind an existing path.
            return Err(StoreError::InvalidInput(
                "workspace path is already bound to a different workspace",
            ));
        }

        self.conn.execute(
            "INSERT INTO workspace_paths(path, workspace, created_at_ms, last_used_at_ms) VALUES (?1, ?2, ?3, ?3)",
            params![path, workspace.as_str(), now_ms],
        )?;
        Ok(())
    }

    /// Return the most-recently used bound path for a workspace (if any).
    pub fn workspace_path_primary_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        match self.conn.query_row(
            "SELECT path FROM workspace_paths WHERE workspace=?1 ORDER BY last_used_at_ms DESC, created_at_ms DESC LIMIT 1",
            params![workspace.as_str()],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) if is_missing_table(&err, "workspace_paths") => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Return a summary of path bindings for the workspace.
    ///
    /// Shape: (primary_path, last_used_at_ms, count).
    pub fn workspace_path_summary_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<(String, i64, i64)>, StoreError> {
        match self.conn.query_row(
            "SELECT path, last_used_at_ms, (SELECT COUNT(*) FROM workspace_paths WHERE workspace=?1) AS cnt \
             FROM workspace_paths \
             WHERE workspace=?1 \
             ORDER BY last_used_at_ms DESC, created_at_ms DESC \
             LIMIT 1",
            params![workspace.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) if is_missing_table(&err, "workspace_paths") => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn workspace_project_guard_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        match self.conn.query_row(
            "SELECT project_guard FROM workspaces WHERE workspace=?1",
            params![workspace.as_str()],
            |row| row.get::<_, Option<String>>(0),
        ) {
            Ok(v) => Ok(Some(v).flatten()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn workspace_project_guard_ensure(
        &mut self,
        workspace: &WorkspaceId,
        expected_guard: &str,
    ) -> Result<(), StoreError> {
        let expected_guard = expected_guard.trim();
        if expected_guard.is_empty() {
            return Err(StoreError::InvalidInput("project_guard must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let stored_guard = tx
            .query_row(
                "SELECT project_guard FROM workspaces WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();

        match stored_guard {
            Some(stored) if stored == expected_guard => {
                tx.commit()?;
                Ok(())
            }
            Some(stored) => Err(StoreError::ProjectGuardMismatch {
                expected: expected_guard.to_string(),
                stored,
            }),
            None => {
                tx.execute(
                    "UPDATE workspaces SET project_guard=?2 WHERE workspace=?1",
                    params![workspace.as_str(), expected_guard],
                )?;
                tx.commit()?;
                Ok(())
            }
        }
    }

    pub fn workspace_project_guard_rebind(
        &mut self,
        workspace: &WorkspaceId,
        expected_guard: &str,
    ) -> Result<(), StoreError> {
        let expected_guard = expected_guard.trim();
        if expected_guard.is_empty() {
            return Err(StoreError::InvalidInput("project_guard must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        tx.execute(
            "UPDATE workspaces SET project_guard=?2 WHERE workspace=?1",
            params![workspace.as_str(), expected_guard],
        )?;
        tx.commit()?;
        Ok(())
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
    ) -> Result<Option<WorkspaceDocEntryHead>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, ts_ms, branch, doc, kind FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
                params![workspace.as_str()],
                |row| {
                    Ok(WorkspaceDocEntryHead {
                        seq: row.get::<_, i64>(0)?,
                        ts_ms: row.get::<_, i64>(1)?,
                        branch: row.get::<_, String>(2)?,
                        doc: row.get::<_, String>(3)?,
                        kind: row.get::<_, String>(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Returns a stable default agent id for this store when the MCP server is launched with
    /// `--agent-id auto` (or `BRANCHMIND_AGENT_ID=auto`).
    ///
    /// The id is persisted in the store-level `meta` table, so it survives process restarts.
    ///
    /// Note: This is a storage-level fallback (per store). Multi-agent isolation still requires
    /// explicit per-agent ids when multiple concurrent agents share the same store.
    pub fn default_agent_id_auto_get_or_create(&mut self) -> Result<String, StoreError> {
        const KEY: &str = "default_agent_id";

        let tx = self.conn.transaction()?;

        let existing: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key=?1", params![KEY], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        if let Some(value) = existing {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(StoreError::InvalidInput(
                    "meta.default_agent_id must not be empty",
                ));
            }
            tx.commit()?;
            return Ok(trimmed.to_string());
        }

        let now_ms = now_ms().max(0) as u64;
        let pid = std::process::id() as u64;
        let candidate = format!("a{}_{}", base36(now_ms), base36(pid));

        // Race-safe: if another process inserted concurrently, we read back what won.
        tx.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES (?1, ?2)",
            params![KEY, candidate],
        )?;

        let stored: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key=?1", params![KEY], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        let stored = stored.unwrap_or_else(|| "self".to_string());
        tx.commit()?;
        Ok(stored)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V3Preflight {
    Empty,
    Compatible,
}

fn v3_preflight_gate(conn: &Connection) -> Result<V3Preflight, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let mut rows = stmt.query([])?;
    let mut tables = Vec::new();
    while let Some(row) = rows.next()? {
        tables.push(row.get::<_, String>(0)?);
    }

    if tables.is_empty() {
        return Ok(V3Preflight::Empty);
    }

    if !tables.iter().any(|name| name == "workspace_state") {
        return Err(StoreError::InvalidInput(
            "RESET_REQUIRED: workspace_state table is missing",
        ));
    }

    let version = conn
        .query_row(
            "SELECT schema_version FROM workspace_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;

    match version {
        Some(v) if v == V3_SCHEMA_VERSION => Ok(V3Preflight::Compatible),
        Some(_) => Err(StoreError::InvalidInput(
            "RESET_REQUIRED: schema version mismatch",
        )),
        None => Err(StoreError::InvalidInput(
            "RESET_REQUIRED: schema state row is missing",
        )),
    }
}

fn install_v3_schema_extensions(conn: &Connection) -> Result<(), StoreError> {
    let now_ms = now_ms();

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workspace_state (
          singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
          schema_version INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS commits (
          workspace TEXT NOT NULL,
          branch TEXT NOT NULL,
          commit_id TEXT NOT NULL,
          parent_commit_id TEXT,
          message TEXT NOT NULL,
          body TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY(workspace, commit_id),
          FOREIGN KEY(workspace, branch)
            REFERENCES branches(workspace, name)
            ON DELETE CASCADE,
          FOREIGN KEY(workspace, parent_commit_id)
            REFERENCES commits(workspace, commit_id)
            ON DELETE RESTRICT,
          CHECK(parent_commit_id IS NULL OR parent_commit_id <> commit_id)
        );

        CREATE INDEX IF NOT EXISTS idx_commits_workspace_branch_created
          ON commits(workspace, branch, created_at_ms, commit_id);

        CREATE TABLE IF NOT EXISTS merge_records (
          workspace TEXT NOT NULL,
          merge_id TEXT NOT NULL,
          source_branch TEXT NOT NULL,
          target_branch TEXT NOT NULL,
          synthesis_commit_id TEXT NOT NULL,
          strategy TEXT NOT NULL,
          summary TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY(workspace, merge_id),
          FOREIGN KEY(workspace, source_branch)
            REFERENCES branches(workspace, name)
            ON DELETE CASCADE,
          FOREIGN KEY(workspace, target_branch)
            REFERENCES branches(workspace, name)
            ON DELETE CASCADE,
          FOREIGN KEY(workspace, synthesis_commit_id)
            REFERENCES commits(workspace, commit_id)
            ON DELETE RESTRICT,
          CHECK(source_branch <> target_branch)
        );

        CREATE INDEX IF NOT EXISTS idx_merge_records_workspace_created
          ON merge_records(workspace, created_at_ms, merge_id);
        ",
    )?;

    if !table_has_column(conn, "branches", "head_commit_id")? {
        conn.execute("ALTER TABLE branches ADD COLUMN head_commit_id TEXT", [])?;
    }
    if !table_has_column(conn, "branches", "updated_at_ms")? {
        conn.execute("ALTER TABLE branches ADD COLUMN updated_at_ms INTEGER", [])?;
    }

    conn.execute(
        "UPDATE branches SET updated_at_ms = COALESCE(updated_at_ms, created_at_ms)",
        [],
    )?;
    conn.execute(
        "INSERT INTO workspace_state(singleton, schema_version, created_at_ms, updated_at_ms) \
         VALUES (1, ?1, ?2, ?2) \
         ON CONFLICT(singleton) DO UPDATE SET \
           schema_version=excluded.schema_version, \
           updated_at_ms=excluded.updated_at_ms",
        params![V3_SCHEMA_VERSION, now_ms],
    )?;

    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, StoreError> {
    let query = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&query)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }

    let mut out = Vec::<u8>::new();
    while value > 0 {
        let idx = (value % 36) as usize;
        out.push(DIGITS[idx]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "0".to_string())
}

#[derive(Clone, Debug)]
struct BranchSource {
    branch: String,
    cutoff_seq: Option<i64>,
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

fn branch_checkout_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
) -> Result<Option<String>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT branch FROM branch_checkout WHERE workspace=?1",
            params![workspace],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

fn branch_checkout_set_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        r#"
        INSERT INTO branch_checkout(workspace, branch, updated_at_ms)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace) DO UPDATE SET branch=excluded.branch, updated_at_ms=excluded.updated_at_ms
        "#,
        params![workspace, branch, now_ms],
    )?;
    Ok(())
}

fn bootstrap_default_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    now_ms: i64,
) -> Result<bool, StoreError> {
    if branch_exists_tx(tx, workspace, DEFAULT_BRANCH)? {
        return Ok(false);
    }
    let base_seq = doc_entries_head_seq_tx(tx, workspace)?.unwrap_or(0);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO branches(workspace, name, base_branch, base_seq, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![workspace, DEFAULT_BRANCH, DEFAULT_BRANCH, base_seq, now_ms],
    )?;
    branch_checkout_set_tx(tx, workspace, DEFAULT_BRANCH, now_ms)?;
    Ok(true)
}

fn ensure_checkout_branch_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    now_ms: i64,
) -> Result<bool, StoreError> {
    if branch_checkout_get_tx(tx, workspace)?.is_some() {
        return Ok(false);
    }
    if !branch_exists_tx(tx, workspace, branch)? {
        return Ok(false);
    }
    branch_checkout_set_tx(tx, workspace, branch, now_ms)?;
    Ok(true)
}
