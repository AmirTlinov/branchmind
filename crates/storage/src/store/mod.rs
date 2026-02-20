#![forbid(unsafe_code)]

mod error;
mod requests;

use bm_core::{MergeRecord, ThoughtBranch, ThoughtCommit, canonical_identifier};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use error::StoreError;
pub use requests::*;

const DB_FILE: &str = "branchmind_rust.db";
const SCHEMA_VERSION: i64 = 3;
const EXPECTED_TABLES: [&str; 4] = ["branches", "commits", "merge_records", "workspace_state"];

#[derive(Debug)]
pub struct SqliteStore {
    storage_dir: PathBuf,
    conn: Connection,
}

impl SqliteStore {
    pub fn open(storage_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&storage_dir)?;

        let db_path = storage_dir.join(DB_FILE);
        let mut conn = Connection::open(db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        ensure_schema_gate(&mut conn)?;

        Ok(Self { storage_dir, conn })
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub fn create_branch(
        &mut self,
        request: CreateBranchRequest,
    ) -> Result<ThoughtBranch, StoreError> {
        let branch = ThoughtBranch::try_new(
            request.workspace_id,
            request.branch_id,
            request.parent_branch_id,
            None,
            request.created_at_ms,
            request.created_at_ms,
        )?;

        let tx = self.conn.transaction()?;
        if let Some(parent_branch_id) = branch.parent_branch_id() {
            ensure_branch_exists_tx(&tx, branch.workspace_id(), parent_branch_id)?;
        }

        let insert_result = tx.execute(
            "INSERT INTO branches(workspace_id, branch_id, parent_branch_id, head_commit_id, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                branch.workspace_id(),
                branch.branch_id(),
                branch.parent_branch_id(),
                branch.head_commit_id(),
                branch.created_at_ms(),
                branch.updated_at_ms()
            ],
        );

        if let Err(err) = insert_result {
            return Err(map_insert_conflict(err, "branch", branch.branch_id()));
        }

        tx.commit()?;
        Ok(branch)
    }

    pub fn list_branches(
        &self,
        request: ListBranchesRequest,
    ) -> Result<Vec<ThoughtBranch>, StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let limit = to_sqlite_i64(request.limit)?;
        let offset = to_sqlite_i64(request.offset)?;

        let mut stmt = self.conn.prepare(
            "SELECT workspace_id, branch_id, parent_branch_id, head_commit_id, created_at_ms, updated_at_ms \
             FROM branches \
             WHERE workspace_id=?1 \
             ORDER BY created_at_ms ASC, branch_id ASC \
             LIMIT ?2 OFFSET ?3",
        )?;

        let mut rows = stmt.query(params![workspace_id, limit, offset])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            out.push(ThoughtBranch::try_new(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            )?);
        }

        Ok(out)
    }

    pub fn delete_branch(&mut self, request: DeleteBranchRequest) -> Result<(), StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let branch_id = canonical_identifier("branch_id", request.branch_id)?;

        let tx = self.conn.transaction()?;
        ensure_branch_exists_tx(&tx, &workspace_id, &branch_id)?;

        tx.execute(
            "DELETE FROM branches WHERE workspace_id=?1 AND branch_id=?2",
            params![workspace_id, branch_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn append_commit(
        &mut self,
        request: AppendCommitRequest,
    ) -> Result<ThoughtCommit, StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let branch_id = canonical_identifier("branch_id", request.branch_id)?;

        let tx = self.conn.transaction()?;
        let branch_state = branch_state_tx(&tx, &workspace_id, &branch_id)?;

        let commit = ThoughtCommit::try_new(
            workspace_id,
            branch_id,
            request.commit_id,
            request.parent_commit_id.or(branch_state.head_commit_id),
            request.message,
            request.body,
            request.created_at_ms,
        )?;

        if let Some(parent_commit_id) = commit.parent_commit_id() {
            ensure_commit_exists_tx(&tx, commit.workspace_id(), parent_commit_id)?;
        }

        let insert_commit = tx.execute(
            "INSERT INTO commits(workspace_id, branch_id, commit_id, parent_commit_id, message, body, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                commit.workspace_id(),
                commit.branch_id(),
                commit.commit_id(),
                commit.parent_commit_id(),
                commit.message(),
                commit.body(),
                commit.created_at_ms()
            ],
        );

        if let Err(err) = insert_commit {
            return Err(map_insert_conflict(err, "commit", commit.commit_id()));
        }

        let updated_at_ms =
            monotonic_updated_at_ms(branch_state.updated_at_ms, commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace_id=?1 AND branch_id=?2",
            params![
                commit.workspace_id(),
                commit.branch_id(),
                commit.commit_id(),
                updated_at_ms
            ],
        )?;

        tx.commit()?;
        Ok(commit)
    }

    pub fn show_commit(
        &self,
        request: ShowCommitRequest,
    ) -> Result<Option<ThoughtCommit>, StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let commit_id = canonical_identifier("commit_id", request.commit_id)?;

        let row = self
            .conn
            .query_row(
                "SELECT workspace_id, branch_id, commit_id, parent_commit_id, message, body, created_at_ms \
                 FROM commits WHERE workspace_id=?1 AND commit_id=?2",
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
                workspace_id,
                branch_id,
                commit_id,
                parent_commit_id,
                message,
                body,
                created_at_ms,
            )) => Ok(Some(ThoughtCommit::try_new(
                workspace_id,
                branch_id,
                commit_id,
                parent_commit_id,
                message,
                body,
                created_at_ms,
            )?)),
            None => Ok(None),
        }
    }

    pub fn create_merge_record(
        &mut self,
        request: CreateMergeRecordRequest,
    ) -> Result<MergeRecord, StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let source_branch_id = canonical_identifier("source_branch_id", request.source_branch_id)?;
        let target_branch_id = canonical_identifier("target_branch_id", request.target_branch_id)?;

        let tx = self.conn.transaction()?;

        ensure_branch_exists_tx(&tx, &workspace_id, &source_branch_id)?;
        let target_state = branch_state_tx(&tx, &workspace_id, &target_branch_id)?;

        let synthesis_commit = ThoughtCommit::try_new(
            workspace_id.clone(),
            target_branch_id.clone(),
            request.synthesis_commit_id,
            target_state.head_commit_id,
            request.synthesis_message,
            request.synthesis_body,
            request.created_at_ms,
        )?;

        if let Some(parent_commit_id) = synthesis_commit.parent_commit_id() {
            ensure_commit_exists_tx(&tx, synthesis_commit.workspace_id(), parent_commit_id)?;
        }

        let merge_record = MergeRecord::try_new(
            workspace_id,
            request.merge_id,
            source_branch_id,
            target_branch_id,
            synthesis_commit.commit_id(),
            request.strategy,
            request.summary,
            request.created_at_ms,
        )?;

        // Atomic write guarantee:
        // commit + merge_record + branch head update all happen in one transaction.
        let insert_commit = tx.execute(
            "INSERT INTO commits(workspace_id, branch_id, commit_id, parent_commit_id, message, body, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                synthesis_commit.workspace_id(),
                synthesis_commit.branch_id(),
                synthesis_commit.commit_id(),
                synthesis_commit.parent_commit_id(),
                synthesis_commit.message(),
                synthesis_commit.body(),
                synthesis_commit.created_at_ms()
            ],
        );

        if let Err(err) = insert_commit {
            return Err(map_insert_conflict(
                err,
                "commit",
                synthesis_commit.commit_id(),
            ));
        }

        let insert_merge = tx.execute(
            "INSERT INTO merge_records(workspace_id, merge_id, source_branch_id, target_branch_id, synthesis_commit_id, strategy, summary, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                merge_record.workspace_id(),
                merge_record.merge_id(),
                merge_record.source_branch_id(),
                merge_record.target_branch_id(),
                merge_record.synthesis_commit_id(),
                merge_record.strategy(),
                merge_record.summary(),
                merge_record.created_at_ms()
            ],
        );

        if let Err(err) = insert_merge {
            return Err(map_insert_conflict(
                err,
                "merge_record",
                merge_record.merge_id(),
            ));
        }

        let updated_at_ms =
            monotonic_updated_at_ms(target_state.updated_at_ms, synthesis_commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace_id=?1 AND branch_id=?2",
            params![
                synthesis_commit.workspace_id(),
                synthesis_commit.branch_id(),
                synthesis_commit.commit_id(),
                updated_at_ms
            ],
        )?;

        tx.commit()?;
        Ok(merge_record)
    }

    pub fn list_merge_records(
        &self,
        request: ListMergeRecordsRequest,
    ) -> Result<Vec<MergeRecord>, StoreError> {
        let workspace_id = canonical_identifier("workspace_id", request.workspace_id)?;
        let limit = to_sqlite_i64(request.limit)?;
        let offset = to_sqlite_i64(request.offset)?;

        let mut stmt = self.conn.prepare(
            "SELECT workspace_id, merge_id, source_branch_id, target_branch_id, synthesis_commit_id, strategy, summary, created_at_ms \
             FROM merge_records \
             WHERE workspace_id=?1 \
             ORDER BY created_at_ms ASC, merge_id ASC \
             LIMIT ?2 OFFSET ?3",
        )?;

        let mut rows = stmt.query(params![workspace_id, limit, offset])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            out.push(MergeRecord::try_new(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            )?);
        }

        Ok(out)
    }
}

fn ensure_schema_gate(conn: &mut Connection) -> Result<(), StoreError> {
    let existing_tables = user_tables(conn)?;
    if existing_tables.is_empty() {
        create_schema(conn)?;
        return Ok(());
    }

    let expected_tables: BTreeSet<String> = EXPECTED_TABLES
        .iter()
        .map(|name| name.to_string())
        .collect();
    if existing_tables != expected_tables {
        let found_schema = read_schema_version(conn).ok().flatten();
        return Err(StoreError::ResetRequired {
            expected_schema: SCHEMA_VERSION,
            found_schema,
            reason: format!(
                "expected v3 tables {:?}, found {:?}",
                expected_tables, existing_tables
            ),
        });
    }

    let found_schema = read_schema_version(conn)?;
    match found_schema {
        Some(version) if version == SCHEMA_VERSION => Ok(()),
        Some(version) => Err(StoreError::ResetRequired {
            expected_schema: SCHEMA_VERSION,
            found_schema: Some(version),
            reason: "schema version mismatch".to_string(),
        }),
        None => Err(StoreError::ResetRequired {
            expected_schema: SCHEMA_VERSION,
            found_schema: None,
            reason: "workspace_state row with schema_version is missing".to_string(),
        }),
    }
}

fn create_schema(conn: &mut Connection) -> Result<(), StoreError> {
    let now_ms = now_ms();
    let tx = conn.transaction()?;

    tx.execute_batch(
        "
        CREATE TABLE workspace_state (
            singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
            schema_version INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE branches (
            workspace_id TEXT NOT NULL,
            branch_id TEXT NOT NULL,
            parent_branch_id TEXT,
            head_commit_id TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(workspace_id, branch_id),
            FOREIGN KEY(workspace_id, parent_branch_id)
                REFERENCES branches(workspace_id, branch_id)
                ON DELETE RESTRICT,
            CHECK(parent_branch_id IS NULL OR parent_branch_id <> branch_id)
        );

        CREATE TABLE commits (
            workspace_id TEXT NOT NULL,
            branch_id TEXT NOT NULL,
            commit_id TEXT NOT NULL,
            parent_commit_id TEXT,
            message TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(workspace_id, commit_id),
            FOREIGN KEY(workspace_id, branch_id)
                REFERENCES branches(workspace_id, branch_id)
                ON DELETE CASCADE,
            FOREIGN KEY(workspace_id, parent_commit_id)
                REFERENCES commits(workspace_id, commit_id)
                ON DELETE RESTRICT,
            CHECK(parent_commit_id IS NULL OR parent_commit_id <> commit_id)
        );

        CREATE TABLE merge_records (
            workspace_id TEXT NOT NULL,
            merge_id TEXT NOT NULL,
            source_branch_id TEXT NOT NULL,
            target_branch_id TEXT NOT NULL,
            synthesis_commit_id TEXT NOT NULL,
            strategy TEXT NOT NULL,
            summary TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(workspace_id, merge_id),
            FOREIGN KEY(workspace_id, source_branch_id)
                REFERENCES branches(workspace_id, branch_id)
                ON DELETE CASCADE,
            FOREIGN KEY(workspace_id, target_branch_id)
                REFERENCES branches(workspace_id, branch_id)
                ON DELETE CASCADE,
            FOREIGN KEY(workspace_id, synthesis_commit_id)
                REFERENCES commits(workspace_id, commit_id)
                ON DELETE RESTRICT,
            CHECK(source_branch_id <> target_branch_id)
        );

        CREATE INDEX idx_branches_workspace_created
            ON branches(workspace_id, created_at_ms, branch_id);

        CREATE INDEX idx_commits_workspace_branch_created
            ON commits(workspace_id, branch_id, created_at_ms, commit_id);

        CREATE INDEX idx_merge_records_workspace_created
            ON merge_records(workspace_id, created_at_ms, merge_id);
        ",
    )?;

    tx.execute(
        "INSERT INTO workspace_state(singleton, schema_version, created_at_ms, updated_at_ms) VALUES (1, ?1, ?2, ?2)",
        params![SCHEMA_VERSION, now_ms],
    )?;

    tx.commit()?;
    Ok(())
}

fn user_tables(conn: &Connection) -> Result<BTreeSet<String>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;

    let mut rows = stmt.query([])?;
    let mut names = BTreeSet::new();
    while let Some(row) = rows.next()? {
        names.insert(row.get::<_, String>(0)?);
    }

    Ok(names)
}

fn read_schema_version(conn: &Connection) -> Result<Option<i64>, StoreError> {
    let has_workspace_state = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_state'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();

    if !has_workspace_state {
        return Ok(None);
    }

    Ok(conn
        .query_row(
            "SELECT schema_version FROM workspace_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

fn ensure_branch_exists_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<(), StoreError> {
    let exists = tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace_id=?1 AND branch_id=?2",
            params![workspace_id, branch_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();

    if exists {
        Ok(())
    } else {
        Err(StoreError::NotFound {
            entity: "branch",
            id: format!("{workspace_id}/{branch_id}"),
        })
    }
}

#[derive(Debug)]
struct BranchState {
    head_commit_id: Option<String>,
    updated_at_ms: i64,
}

fn branch_state_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<BranchState, StoreError> {
    let value = tx
        .query_row(
            "SELECT head_commit_id, updated_at_ms FROM branches WHERE workspace_id=?1 AND branch_id=?2",
            params![workspace_id, branch_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    match value {
        Some((head_commit_id, updated_at_ms)) => Ok(BranchState {
            head_commit_id,
            updated_at_ms,
        }),
        None => Err(StoreError::NotFound {
            entity: "branch",
            id: format!("{workspace_id}/{branch_id}"),
        }),
    }
}

fn monotonic_updated_at_ms(previous_updated_at_ms: i64, candidate_updated_at_ms: i64) -> i64 {
    previous_updated_at_ms.max(candidate_updated_at_ms)
}

fn ensure_commit_exists_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    commit_id: &str,
) -> Result<(), StoreError> {
    let exists = tx
        .query_row(
            "SELECT 1 FROM commits WHERE workspace_id=?1 AND commit_id=?2",
            params![workspace_id, commit_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();

    if exists {
        Ok(())
    } else {
        Err(StoreError::NotFound {
            entity: "commit",
            id: format!("{workspace_id}/{commit_id}"),
        })
    }
}

fn map_insert_conflict(err: rusqlite::Error, entity: &'static str, id: &str) -> StoreError {
    if is_constraint_violation(&err) {
        return StoreError::AlreadyExists {
            entity,
            id: id.to_string(),
        };
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
    i64::try_from(value).map_err(|_| StoreError::InvalidInput("value is too large"))
}

fn now_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
