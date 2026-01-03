#![forbid(unsafe_code)]
//! Storage implementation (split-friendly module root).

mod branches;
mod docs;
mod error;
mod focus;
mod graph;
mod ops_history;
mod reasoning_ref;
mod steps;
mod support;
mod tasks;
mod think;
mod types;
mod vcs;

use bm_core::ids::WorkspaceId;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};

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
        let store = Self { storage_dir, conn };
        store.migrate()?;
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
