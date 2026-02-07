#![forbid(unsafe_code)]
//! Storage implementation (split-friendly module root).

mod anchor_aliases;
mod anchor_links;
mod anchors;
mod anchors_lint;
mod anchors_merge;
mod branches;
mod docs;
mod error;
mod focus;
mod graph;
mod jobs;
mod knowledge_keys;
mod ops_history;
mod portal_cursors;
mod reasoning_ref;
mod runners;
mod steps;
mod support;
mod tasks;
mod think;
mod types;
mod vcs;

use bm_core::ids::WorkspaceId;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_BRANCH: &str = "main";

pub use error::StoreError;
pub use types::*;

use support::*;

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
        let store = Self { storage_dir, conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open an existing store in read-only mode.
    ///
    /// Notes:
    /// - This does not create directories or run migrations.
    /// - Intended for passive read-only consumers (e.g. offline inspection tooling reading
    ///   other projects via the registry).
    pub fn open_read_only(storage_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        let db_path = storage_dir.join("branchmind_rust.db");
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(Self { storage_dir, conn })
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

    pub fn workspace_project_guard_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT project_guard FROM workspaces WHERE workspace=?1",
                params![workspace.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
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
