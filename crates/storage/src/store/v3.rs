#![forbid(unsafe_code)]

use super::*;
use bm_core::{MergeRecord, ThoughtBranch, ThoughtCommit, canonical_identifier, ids::WorkspaceId};
use rusqlite::{ErrorCode, OptionalExtension, Transaction, params};

impl SqliteStore {
    pub fn create_branch(
        &mut self,
        request: CreateBranchRequest,
    ) -> Result<ThoughtBranch, StoreError> {
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let branch_id = canonicalize_identifier("branch_id", request.branch_id)?;
        let parent_branch_id = request
            .parent_branch_id
            .map(|value| canonicalize_identifier("parent_branch_id", value))
            .transpose()?;

        let tx = self.conn.transaction()?;
        let workspace = WorkspaceId::try_new(workspace_id.clone())
            .map_err(|_| StoreError::InvalidInput("invalid workspace_id"))?;
        ensure_workspace_tx(&tx, &workspace, request.created_at_ms)?;

        let parent_head_commit_id = if let Some(parent_branch_id) = parent_branch_id.as_deref() {
            let parent_state = branch_state_v3_tx(&tx, &workspace_id, parent_branch_id)?;
            Some(parent_state.head_commit_id)
        } else {
            None
        }
        .flatten();

        let branch = ThoughtBranch::try_new(
            workspace_id,
            branch_id,
            parent_branch_id,
            parent_head_commit_id,
            request.created_at_ms,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid v3 branch payload"))?;

        let base_branch = branch.parent_branch_id().unwrap_or(branch.branch_id());
        let insert_result = tx.execute(
            "INSERT INTO branches(workspace, name, base_branch, base_seq, created_at_ms, head_commit_id, updated_at_ms) \
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            params![
                branch.workspace_id(),
                branch.branch_id(),
                base_branch,
                branch.created_at_ms(),
                branch.head_commit_id(),
                branch.updated_at_ms()
            ],
        );

        if let Err(err) = insert_result {
            return Err(map_insert_conflict_v3(err));
        }

        tx.commit()?;
        Ok(branch)
    }

    pub fn list_branches(
        &self,
        request: ListBranchesRequest,
    ) -> Result<Vec<ThoughtBranch>, StoreError> {
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let limit = to_sqlite_i64(request.limit)?;
        let offset = to_sqlite_i64(request.offset)?;

        let mut stmt = self.conn.prepare(
            "SELECT workspace, name, base_branch, head_commit_id, created_at_ms, COALESCE(updated_at_ms, created_at_ms) AS updated_at_ms \
             FROM branches \
             WHERE workspace=?1 \
             ORDER BY created_at_ms ASC, name ASC \
             LIMIT ?2 OFFSET ?3",
        )?;

        let mut rows = stmt.query(params![workspace_id, limit, offset])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            let branch_id = row.get::<_, String>(1)?;
            let base_branch = row.get::<_, String>(2)?;
            let parent_branch_id = if base_branch == branch_id {
                None
            } else {
                Some(base_branch)
            };

            out.push(
                ThoughtBranch::try_new(
                    row.get::<_, String>(0)?,
                    branch_id,
                    parent_branch_id,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                )
                .map_err(|_| StoreError::InvalidInput("invalid v3 branch row"))?,
            );
        }

        Ok(out)
    }

    pub fn delete_branch(&mut self, request: DeleteBranchRequest) -> Result<(), StoreError> {
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let branch_id = canonicalize_identifier("branch_id", request.branch_id)?;

        let tx = self.conn.transaction()?;
        ensure_branch_exists_v3_tx(&tx, &workspace_id, &branch_id)?;

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
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let branch_id = canonicalize_identifier("branch_id", request.branch_id)?;

        let tx = self.conn.transaction()?;
        let branch_state = branch_state_v3_tx(&tx, &workspace_id, &branch_id)?;

        let commit = ThoughtCommit::try_new(
            workspace_id,
            branch_id,
            request.commit_id,
            request.parent_commit_id.or(branch_state.head_commit_id),
            request.message,
            request.body,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid v3 commit payload"))?;

        if let Some(parent_commit_id) = commit.parent_commit_id() {
            ensure_commit_exists_v3_tx(&tx, commit.workspace_id(), parent_commit_id)?;
        }

        let insert_commit = tx.execute(
            "INSERT INTO commits(workspace, branch, commit_id, parent_commit_id, message, body, created_at_ms) \
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
            return Err(map_insert_conflict_v3(err));
        }

        let updated_at_ms =
            monotonic_updated_at_ms(branch_state.updated_at_ms, commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace=?1 AND name=?2",
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
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let commit_id = canonicalize_identifier("commit_id", request.commit_id)?;

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
                .map_err(|_| StoreError::InvalidInput("invalid v3 commit row"))?,
            )),
            None => Ok(None),
        }
    }

    pub fn create_merge_record(
        &mut self,
        request: CreateMergeRecordRequest,
    ) -> Result<MergeRecord, StoreError> {
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
        let source_branch = canonicalize_identifier("source_branch_id", request.source_branch_id)?;
        let target_branch = canonicalize_identifier("target_branch_id", request.target_branch_id)?;

        let tx = self.conn.transaction()?;

        ensure_branch_exists_v3_tx(&tx, &workspace_id, &source_branch)?;
        let target_state = branch_state_v3_tx(&tx, &workspace_id, &target_branch)?;

        let synthesis_commit = ThoughtCommit::try_new(
            workspace_id.clone(),
            target_branch.clone(),
            request.synthesis_commit_id,
            target_state.head_commit_id,
            request.synthesis_message,
            request.synthesis_body,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid synthesis commit payload"))?;

        if let Some(parent_commit_id) = synthesis_commit.parent_commit_id() {
            ensure_commit_exists_v3_tx(&tx, synthesis_commit.workspace_id(), parent_commit_id)?;
        }

        let merge_record = MergeRecord::try_new(
            workspace_id,
            request.merge_id,
            source_branch,
            target_branch,
            synthesis_commit.commit_id(),
            request.strategy,
            request.summary,
            request.created_at_ms,
        )
        .map_err(|_| StoreError::InvalidInput("invalid merge record payload"))?;

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
                synthesis_commit.created_at_ms()
            ],
        );

        if let Err(err) = insert_commit {
            return Err(map_insert_conflict_v3(err));
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
                merge_record.created_at_ms()
            ],
        );

        if let Err(err) = insert_merge {
            return Err(map_insert_conflict_v3(err));
        }

        let updated_at_ms =
            monotonic_updated_at_ms(target_state.updated_at_ms, synthesis_commit.created_at_ms());
        tx.execute(
            "UPDATE branches SET head_commit_id=?3, updated_at_ms=?4 WHERE workspace=?1 AND name=?2",
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
        let workspace_id = canonicalize_identifier("workspace_id", request.workspace_id)?;
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
}

#[derive(Debug)]
struct BranchStateV3 {
    head_commit_id: Option<String>,
    updated_at_ms: i64,
}

fn branch_state_v3_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<BranchStateV3, StoreError> {
    let value = tx
        .query_row(
            "SELECT head_commit_id, COALESCE(updated_at_ms, created_at_ms) FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace_id, branch_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    match value {
        Some((head_commit_id, updated_at_ms)) => Ok(BranchStateV3 {
            head_commit_id,
            updated_at_ms,
        }),
        None => Err(StoreError::UnknownId),
    }
}

fn ensure_branch_exists_v3_tx(
    tx: &Transaction<'_>,
    workspace_id: &str,
    branch_id: &str,
) -> Result<(), StoreError> {
    let exists = tx
        .query_row(
            "SELECT 1 FROM branches WHERE workspace=?1 AND name=?2",
            params![workspace_id, branch_id],
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

fn ensure_commit_exists_v3_tx(
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

fn map_insert_conflict_v3(err: rusqlite::Error) -> StoreError {
    if is_constraint_violation_v3(&err) {
        return StoreError::BranchAlreadyExists;
    }
    StoreError::Sql(err)
}

fn is_constraint_violation_v3(err: &rusqlite::Error) -> bool {
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

fn monotonic_updated_at_ms(previous_updated_at_ms: i64, candidate_updated_at_ms: i64) -> i64 {
    previous_updated_at_ms.max(candidate_updated_at_ms)
}

fn to_sqlite_i64(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::InvalidInput("value is too large"))
}

fn canonicalize_identifier(field: &'static str, value: String) -> Result<String, StoreError> {
    canonical_identifier(field, value).map_err(|_| StoreError::InvalidInput("invalid identifier"))
}
