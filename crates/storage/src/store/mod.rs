#![forbid(unsafe_code)]

mod error;
mod requests;

pub use error::StoreError;
pub use requests::*;

use bm_core::{MergeRecord, ThoughtBranch, ThoughtCommit, canonical_identifier, ids::WorkspaceId};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_BRANCH: &str = "main";
const V3_SCHEMA_VERSION: i64 = 3;
const MAX_BRANCH_DEPTH: usize = 128;

#[derive(Debug)]
pub struct SqliteStore {
    conn: Connection,
    storage_dir: PathBuf,
}

impl SqliteStore {
    pub fn open(storage_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&storage_dir)?;

        let db_path = storage_dir.join("branchmind_rust.db");
        let conn = Connection::open(db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        preflight_gate(&conn)?;
        install_schema(&conn)?;

        Ok(Self { conn, storage_dir })
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub fn default_branch_name(&self) -> &'static str {
        DEFAULT_BRANCH
    }

    pub fn create_branch(
        &mut self,
        request: CreateBranchRequest,
    ) -> Result<ThoughtBranch, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let branch_id = canonicalize_branch(&request.branch_id)?;
        let parent_branch_id = request
            .parent_branch_id
            .as_deref()
            .map(canonicalize_branch)
            .transpose()?;

        if parent_branch_id
            .as_ref()
            .is_some_and(|parent| parent == &branch_id)
        {
            return Err(StoreError::BranchCycle);
        }

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, &workspace_id, request.created_at_ms)?;

        let parent_head_commit_id = if let Some(parent_branch_id) = parent_branch_id.as_deref() {
            let state = branch_state_tx(&tx, &workspace_id, parent_branch_id)?;
            let depth = branch_depth_tx(&tx, &workspace_id, parent_branch_id)?;
            if depth + 1 > MAX_BRANCH_DEPTH {
                return Err(StoreError::BranchDepthExceeded);
            }
            state.head_commit_id
        } else {
            None
        };

        let branch = ThoughtBranch::try_new(
            workspace_id.clone(),
            branch_id.clone(),
            parent_branch_id.clone(),
            parent_head_commit_id,
            request.created_at_ms,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid branch payload"))?;

        let insert = tx.execute(
            "INSERT INTO branches(workspace, name, parent_branch_id, head_commit_id, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                branch.workspace_id(),
                branch.branch_id(),
                branch.parent_branch_id(),
                branch.head_commit_id(),
                branch.created_at_ms(),
                branch.updated_at_ms(),
            ],
        );

        if let Err(err) = insert {
            return Err(map_insert_conflict(err));
        }

        tx.commit()?;
        Ok(branch)
    }

    pub fn list_branches(
        &self,
        request: ListBranchesRequest,
    ) -> Result<Vec<ThoughtBranch>, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let limit = to_sqlite_i64(request.limit)?;
        let offset = to_sqlite_i64(request.offset)?;

        let mut stmt = self.conn.prepare(
            "SELECT workspace, name, parent_branch_id, head_commit_id, created_at_ms, updated_at_ms \
             FROM branches \
             WHERE workspace=?1 \
             ORDER BY created_at_ms ASC, name ASC \
             LIMIT ?2 OFFSET ?3",
        )?;

        let mut rows = stmt.query(params![workspace_id, limit, offset])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            out.push(
                ThoughtBranch::try_new(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                )
                .map_err(|_| StoreError::InvalidInput("invalid branch row"))?,
            );
        }

        Ok(out)
    }

    pub fn delete_branch(&mut self, request: DeleteBranchRequest) -> Result<(), StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let branch_id = canonicalize_branch(&request.branch_id)?;

        let tx = self.conn.transaction()?;
        ensure_branch_exists_tx(&tx, &workspace_id, &branch_id)?;

        let descendants = tx.query_row(
            "SELECT COUNT(1) FROM branches WHERE workspace=?1 AND parent_branch_id=?2",
            params![workspace_id, branch_id],
            |row| row.get::<_, i64>(0),
        )?;

        if descendants > 0 {
            return Err(StoreError::InvalidInput(
                "branch has descendants and cannot be deleted",
            ));
        }

        tx.execute(
            "DELETE FROM merge_records WHERE workspace=?1 AND (source_branch=?2 OR target_branch=?2)",
            params![workspace_id, branch_id],
        )?;

        delete_branch_commits_tx(&tx, &workspace_id, &branch_id)?;

        tx.execute(
            "DELETE FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace_id, branch_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn append_commit(
        &mut self,
        request: AppendCommitRequest,
    ) -> Result<ThoughtCommit, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let branch_id = canonicalize_branch(&request.branch_id)?;
        let commit_id = canonicalize_commit(&request.commit_id)?;
        let explicit_parent = request
            .parent_commit_id
            .as_deref()
            .map(canonicalize_commit)
            .transpose()?;

        let tx = self.conn.transaction()?;
        let branch_state = branch_state_tx(&tx, &workspace_id, &branch_id)?;

        let parent_commit_id = explicit_parent.or(branch_state.head_commit_id);
        if let Some(parent_commit_id) = parent_commit_id.as_deref() {
            ensure_commit_exists_tx(&tx, &workspace_id, parent_commit_id)?;
            ensure_commit_belongs_to_branch_tx(&tx, &workspace_id, parent_commit_id, &branch_id)?;
        }

        let commit = ThoughtCommit::try_new(
            workspace_id,
            branch_id,
            commit_id,
            parent_commit_id,
            request.message,
            request.body,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid commit payload"))?;

        let insert = tx.execute(
            "INSERT INTO commits(workspace, branch, commit_id, parent_commit_id, message, body, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                commit.workspace_id(),
                commit.branch_id(),
                commit.commit_id(),
                commit.parent_commit_id(),
                commit.message(),
                commit.body(),
                commit.created_at_ms(),
            ],
        );

        if let Err(err) = insert {
            return Err(map_insert_conflict(err));
        }

        let updated_at_ms = branch_state.updated_at_ms.max(commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace=?1 AND name=?2",
            params![
                commit.workspace_id(),
                commit.branch_id(),
                commit.commit_id(),
                updated_at_ms,
            ],
        )?;

        tx.commit()?;
        Ok(commit)
    }

    pub fn show_commit(
        &self,
        request: ShowCommitRequest,
    ) -> Result<Option<ThoughtCommit>, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let commit_id = canonicalize_commit(&request.commit_id)?;

        let row = self
            .conn
            .query_row(
                "SELECT workspace, branch, commit_id, parent_commit_id, message, body, created_at_ms \
                 FROM commits WHERE workspace=?1 AND commit_id=?2",
                params![workspace_id, commit_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((
                workspace,
                branch,
                commit_id,
                parent_commit_id,
                message,
                body,
                created_at_ms,
            )) => Ok(Some(
                ThoughtCommit::try_new(
                    workspace,
                    branch,
                    commit_id,
                    parent_commit_id,
                    message,
                    body,
                    created_at_ms,
                )
                .map_err(|_| StoreError::InvalidInput("invalid commit row"))?,
            )),
            None => Ok(None),
        }
    }

    pub fn create_merge_record(
        &mut self,
        request: CreateMergeRecordRequest,
    ) -> Result<MergeRecord, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let source_branch_id = canonicalize_branch(&request.source_branch_id)?;
        let target_branch_id = canonicalize_branch(&request.target_branch_id)?;
        let merge_id = canonicalize_merge(&request.merge_id)?;
        let synthesis_commit_id = canonicalize_commit(&request.synthesis_commit_id)?;

        let tx = self.conn.transaction()?;
        ensure_branch_exists_tx(&tx, &workspace_id, &source_branch_id)?;
        let target_state = branch_state_tx(&tx, &workspace_id, &target_branch_id)?;

        let synthesis_commit = ThoughtCommit::try_new(
            workspace_id.clone(),
            target_branch_id.clone(),
            synthesis_commit_id,
            target_state.head_commit_id,
            request.synthesis_message,
            request.synthesis_body,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid synthesis commit payload"))?;

        if let Some(parent_commit_id) = synthesis_commit.parent_commit_id() {
            ensure_commit_exists_tx(&tx, &workspace_id, parent_commit_id)?;
            ensure_commit_belongs_to_branch_tx(
                &tx,
                &workspace_id,
                parent_commit_id,
                &target_branch_id,
            )?;
        }

        let merge_record = MergeRecord::try_new(
            workspace_id,
            merge_id,
            source_branch_id,
            target_branch_id,
            synthesis_commit.commit_id(),
            request.strategy,
            request.summary,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid merge payload"))?;

        let insert_commit = tx.execute(
            "INSERT INTO commits(workspace, branch, commit_id, parent_commit_id, message, body, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                synthesis_commit.workspace_id(),
                synthesis_commit.branch_id(),
                synthesis_commit.commit_id(),
                synthesis_commit.parent_commit_id(),
                synthesis_commit.message(),
                synthesis_commit.body(),
                synthesis_commit.created_at_ms(),
            ],
        );

        if let Err(err) = insert_commit {
            return Err(map_insert_conflict(err));
        }

        let insert_merge = tx.execute(
            "INSERT INTO merge_records(workspace, merge_id, source_branch, target_branch, synthesis_commit_id, strategy, summary, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                merge_record.workspace_id(),
                merge_record.merge_id(),
                merge_record.source_branch_id(),
                merge_record.target_branch_id(),
                merge_record.synthesis_commit_id(),
                merge_record.strategy(),
                merge_record.summary(),
                merge_record.created_at_ms(),
            ],
        );

        if let Err(err) = insert_merge {
            return Err(map_insert_conflict(err));
        }

        let updated_at_ms = target_state
            .updated_at_ms
            .max(synthesis_commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace=?1 AND name=?2",
            params![
                synthesis_commit.workspace_id(),
                synthesis_commit.branch_id(),
                synthesis_commit.commit_id(),
                updated_at_ms,
            ],
        )?;

        tx.commit()?;
        Ok(merge_record)
    }

    pub fn list_merge_records(
        &self,
        request: ListMergeRecordsRequest,
    ) -> Result<Vec<MergeRecord>, StoreError> {
        let workspace_id = canonicalize_workspace(&request.workspace_id)?;
        let limit = to_sqlite_i64(request.limit)?;
        let offset = to_sqlite_i64(request.offset)?;

        let mut stmt = self.conn.prepare(
            "SELECT workspace, merge_id, source_branch, target_branch, synthesis_commit_id, strategy, summary, created_at_ms \
             FROM merge_records \
             WHERE workspace=?1 \
             ORDER BY created_at_ms ASC, merge_id ASC \
             LIMIT ?2 OFFSET ?3",
        )?;

        let mut rows = stmt.query(params![workspace_id, limit, offset])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            out.push(
                MergeRecord::try_new(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                )
                .map_err(|_| StoreError::InvalidInput("invalid merge row"))?,
            );
        }

        Ok(out)
    }

    pub fn branch_exists(&self, workspace: &WorkspaceId, branch: &str) -> Result<bool, StoreError> {
        let workspace_id = canonicalize_workspace(workspace.as_str())?;
        let branch_id = canonicalize_branch(branch)?;
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace_id, branch_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    }

    pub fn branch_checkout_get(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Option<String>, StoreError> {
        let workspace_id = canonicalize_workspace(workspace.as_str())?;
        Ok(self
            .conn
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn branch_checkout_set(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
    ) -> Result<(Option<String>, String), StoreError> {
        let workspace_id = canonicalize_workspace(workspace.as_str())?;
        let branch_id = canonicalize_branch(branch)?;
        let now_ms = now_ms();

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, &workspace_id, now_ms)?;
        if !branch_exists_tx(&tx, &workspace_id, &branch_id)? {
            return Err(StoreError::UnknownBranch);
        }

        let previous = tx
            .query_row(
                "SELECT branch FROM branch_checkout WHERE workspace=?1",
                params![workspace_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        tx.execute(
            r#"
            INSERT INTO branch_checkout(workspace, branch, updated_at_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(workspace) DO UPDATE SET branch=excluded.branch, updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace_id, branch_id, now_ms],
        )?;

        tx.commit()?;
        Ok((previous, branch_id))
    }
}

#[derive(Debug)]
struct BranchState {
    head_commit_id: Option<String>,
    updated_at_ms: i64,
}

fn preflight_gate(conn: &Connection) -> Result<(), StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let mut rows = stmt.query([])?;
    let mut tables = BTreeSet::new();
    while let Some(row) = rows.next()? {
        tables.insert(row.get::<_, String>(0)?);
    }

    if tables.is_empty() {
        return Ok(());
    }

    let required: BTreeSet<&str> = [
        "workspace_state",
        "workspaces",
        "branches",
        "branch_checkout",
        "commits",
        "merge_records",
    ]
    .into_iter()
    .collect();

    if tables
        .iter()
        .any(|table| !required.contains(table.as_str()))
    {
        return Err(StoreError::InvalidInput(
            "RESET_REQUIRED: unsupported tables detected",
        ));
    }

    for table in required {
        if !tables.contains(table) {
            return Err(StoreError::InvalidInput(
                "RESET_REQUIRED: required table is missing",
            ));
        }
    }

    let version = conn
        .query_row(
            "SELECT schema_version FROM workspace_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;

    match version {
        Some(v) if v == V3_SCHEMA_VERSION => Ok(()),
        Some(_) => Err(StoreError::InvalidInput(
            "RESET_REQUIRED: schema version mismatch",
        )),
        None => Err(StoreError::InvalidInput(
            "RESET_REQUIRED: schema state row is missing",
        )),
    }
}

fn install_schema(conn: &Connection) -> Result<(), StoreError> {
    let now_ms = now_ms();

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS workspace_state (
          singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
          schema_version INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspaces (
          workspace TEXT PRIMARY KEY,
          created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS branches (
          workspace TEXT NOT NULL,
          name TEXT NOT NULL,
          parent_branch_id TEXT,
          head_commit_id TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY(workspace, name),
          FOREIGN KEY(workspace) REFERENCES workspaces(workspace) ON DELETE CASCADE,
          FOREIGN KEY(workspace, parent_branch_id)
            REFERENCES branches(workspace, name)
            ON DELETE RESTRICT,
          CHECK(parent_branch_id IS NULL OR parent_branch_id <> name)
        );

        CREATE INDEX IF NOT EXISTS idx_branches_workspace_created
          ON branches(workspace, created_at_ms, name);

        CREATE TABLE IF NOT EXISTS branch_checkout (
          workspace TEXT PRIMARY KEY,
          branch TEXT NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          FOREIGN KEY(workspace, branch)
            REFERENCES branches(workspace, name)
            ON DELETE CASCADE
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
        "#,
    )?;

    conn.execute(
        "INSERT INTO workspace_state(singleton, schema_version, created_at_ms, updated_at_ms) \
         VALUES (1, ?1, ?2, ?2) \
         ON CONFLICT(singleton) DO UPDATE SET schema_version=excluded.schema_version, updated_at_ms=excluded.updated_at_ms",
        params![V3_SCHEMA_VERSION, now_ms],
    )?;

    Ok(())
}

fn ensure_workspace_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    now_ms: i64,
) -> Result<(), StoreError> {
    tx.execute(
        "INSERT OR IGNORE INTO workspaces(workspace, created_at_ms) VALUES (?1, ?2)",
        params![workspace_id, now_ms],
    )?;
    Ok(())
}

fn branch_exists_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<bool, StoreError> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace_id, branch_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

fn ensure_branch_exists_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<(), StoreError> {
    if branch_exists_tx(tx, workspace_id, branch_id)? {
        Ok(())
    } else {
        Err(StoreError::UnknownId)
    }
}

fn branch_state_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<BranchState, StoreError> {
    let value = tx
        .query_row(
            "SELECT head_commit_id, updated_at_ms FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace_id, branch_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    match value {
        Some((head_commit_id, updated_at_ms)) => Ok(BranchState {
            head_commit_id,
            updated_at_ms,
        }),
        None => Err(StoreError::UnknownId),
    }
}

fn branch_depth_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<usize, StoreError> {
    let mut current = Some(branch_id.to_string());
    let mut depth = 0usize;
    let mut seen = BTreeSet::new();

    while let Some(branch) = current {
        if !seen.insert(branch.clone()) {
            return Err(StoreError::BranchCycle);
        }

        let parent = tx
            .query_row(
                "SELECT parent_branch_id FROM branches WHERE workspace=?1 AND name=?2",
                params![workspace_id, branch],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();

        current = parent;
        if current.is_some() {
            depth = depth.saturating_add(1);
            if depth > MAX_BRANCH_DEPTH {
                return Err(StoreError::BranchDepthExceeded);
            }
        }
    }

    Ok(depth)
}

fn ensure_commit_exists_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    commit_id: &str,
) -> Result<(), StoreError> {
    let exists = tx
        .query_row(
            "SELECT 1 FROM commits WHERE workspace=?1 AND commit_id=?2",
            params![workspace_id, commit_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();

    if exists {
        Ok(())
    } else {
        Err(StoreError::UnknownId)
    }
}

fn ensure_commit_belongs_to_branch_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    commit_id: &str,
    branch_id: &str,
) -> Result<(), StoreError> {
    let belongs = tx
        .query_row(
            "SELECT 1 FROM commits WHERE workspace=?1 AND commit_id=?2 AND branch=?3",
            params![workspace_id, commit_id, branch_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();

    if belongs {
        Ok(())
    } else {
        Err(StoreError::InvalidInput(
            "parent commit must belong to the same branch",
        ))
    }
}

fn delete_branch_commits_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<(), StoreError> {
    loop {
        let deleted = tx.execute(
            "DELETE FROM commits \
             WHERE workspace=?1 AND branch=?2 \
               AND commit_id NOT IN ( \
                   SELECT parent_commit_id \
                   FROM commits \
                   WHERE workspace=?1 AND branch=?2 AND parent_commit_id IS NOT NULL \
               )",
            params![workspace_id, branch_id],
        )?;

        if deleted == 0 {
            break;
        }
    }
    Ok(())
}

fn map_insert_conflict(err: rusqlite::Error) -> StoreError {
    if is_constraint_violation(&err) {
        return StoreError::BranchAlreadyExists;
    }
    StoreError::Sql(err)
}

fn is_constraint_violation(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(code, message) => {
            code.code == ErrorCode::ConstraintViolation
                || message.as_deref().is_some_and(|value| {
                    value.contains("UNIQUE constraint failed")
                        || value.contains("PRIMARY KEY constraint failed")
                })
        }
        _ => false,
    }
}

fn to_sqlite_i64(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::InvalidInput("numeric overflow"))
}

fn canonicalize_workspace(value: &str) -> Result<String, StoreError> {
    canonical_identifier("workspace_id", value.to_string())
        .map_err(|_| StoreError::InvalidInput("invalid workspace_id"))
}

fn canonicalize_branch(value: &str) -> Result<String, StoreError> {
    canonical_identifier("branch_id", value.to_string())
        .map_err(|_| StoreError::InvalidInput("invalid branch_id"))
}

fn canonicalize_commit(value: &str) -> Result<String, StoreError> {
    canonical_identifier("commit_id", value.to_string())
        .map_err(|_| StoreError::InvalidInput("invalid commit_id"))
}

fn canonicalize_merge(value: &str) -> Result<String, StoreError> {
    canonical_identifier("merge_id", value.to_string())
        .map_err(|_| StoreError::InvalidInput("invalid merge_id"))
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration,
        Err(_) => return 0,
    };

    i64::try_from(now.as_millis()).unwrap_or(i64::MAX)
}
