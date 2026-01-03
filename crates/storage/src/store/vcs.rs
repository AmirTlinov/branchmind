#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn vcs_ref_get(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        doc: &str,
    ) -> Result<Option<VcsRefRow>, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let tx = self.conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT branch, seq, updated_at_ms FROM vcs_refs WHERE workspace=?1 AND ref=?2 AND doc=?3",
                params![workspace.as_str(), reference, doc],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()?;
        tx.commit()?;
        Ok(row.map(|(branch, seq, updated_at_ms)| VcsRefRow {
            reference: reference.to_string(),
            branch,
            doc: doc.to_string(),
            seq,
            updated_at_ms,
        }))
    }

    pub fn vcs_ref_set(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        branch: &str,
        doc: &str,
        seq: i64,
        message: Option<String>,
    ) -> Result<VcsRefUpdate, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if seq <= 0 {
            return Err(StoreError::InvalidInput("seq must be positive"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let existing = tx
            .query_row(
                "SELECT branch, seq, updated_at_ms FROM vcs_refs WHERE workspace=?1 AND ref=?2 AND doc=?3",
                params![workspace.as_str(), reference, doc],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()?;

        let old_seq = existing.as_ref().map(|(_, seq, _)| *seq);
        let existing_branch = existing.as_ref().map(|(branch, _, _)| branch.as_str());
        let needs_update = existing_branch != Some(branch) || old_seq != Some(seq);

        if needs_update {
            tx.execute(
                r#"
                INSERT INTO vcs_refs(workspace, ref, doc, branch, seq, updated_at_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(workspace, ref, doc) DO UPDATE SET
                  branch=excluded.branch,
                  seq=excluded.seq,
                  updated_at_ms=excluded.updated_at_ms
                "#,
                params![workspace.as_str(), reference, doc, branch, seq, now_ms],
            )?;

            tx.execute(
                r#"
                INSERT INTO vcs_reflog(workspace, ref, doc, branch, old_seq, new_seq, message, ts_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    workspace.as_str(),
                    reference,
                    doc,
                    branch,
                    old_seq,
                    seq,
                    message.as_deref(),
                    now_ms
                ],
            )?;
        }

        tx.commit()?;
        Ok(VcsRefUpdate {
            reference: VcsRefRow {
                reference: reference.to_string(),
                branch: branch.to_string(),
                doc: doc.to_string(),
                seq,
                updated_at_ms: now_ms,
            },
            old_seq,
        })
    }

    pub fn vcs_reflog_list(
        &mut self,
        workspace: &WorkspaceId,
        reference: &str,
        doc: &str,
        limit: usize,
    ) -> Result<Vec<VcsReflogRow>, StoreError> {
        if reference.trim().is_empty() {
            return Err(StoreError::InvalidInput("ref must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;
        let out = {
            let mut stmt = tx.prepare(
                "SELECT branch, old_seq, new_seq, message, ts_ms \
                 FROM vcs_reflog WHERE workspace=?1 AND ref=?2 AND doc=?3 \
                 ORDER BY ts_ms DESC, new_seq DESC LIMIT ?4",
            )?;
            let mut rows = stmt.query(params![workspace.as_str(), reference, doc, limit])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(VcsReflogRow {
                    reference: reference.to_string(),
                    branch: row.get(0)?,
                    doc: doc.to_string(),
                    old_seq: row.get(1)?,
                    new_seq: row.get(2)?,
                    message: row.get(3)?,
                    ts_ms: row.get(4)?,
                });
            }
            out
        };
        tx.commit()?;
        Ok(out)
    }

    pub fn vcs_tag_get(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<Option<VcsTagRow>, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        let tx = self.conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT branch, doc, seq, created_at_ms FROM vcs_tags WHERE workspace=?1 AND name=?2",
                params![workspace.as_str(), name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
            )
            .optional()?;
        tx.commit()?;
        Ok(row.map(|(branch, doc, seq, created_at_ms)| VcsTagRow {
            name: name.to_string(),
            branch,
            doc,
            seq,
            created_at_ms,
        }))
    }

    pub fn vcs_tag_create(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
        branch: &str,
        doc: &str,
        seq: i64,
        force: bool,
    ) -> Result<VcsTagRow, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if seq <= 0 {
            return Err(StoreError::InvalidInput("seq must be positive"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;
        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }
        if !doc_entry_visible_tx(&tx, workspace.as_str(), branch, doc, seq)? {
            return Err(StoreError::InvalidInput("commit not visible for branch"));
        }

        if !force {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM vcs_tags WHERE workspace=?1 AND name=?2 LIMIT 1",
                    params![workspace.as_str(), name],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                return Err(StoreError::InvalidInput("tag already exists"));
            }
        }

        tx.execute(
            r#"
            INSERT INTO vcs_tags(workspace, name, doc, branch, seq, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(workspace, name) DO UPDATE SET
              doc=excluded.doc,
              branch=excluded.branch,
              seq=excluded.seq,
              created_at_ms=excluded.created_at_ms
            "#,
            params![workspace.as_str(), name, doc, branch, seq, now_ms],
        )?;
        tx.commit()?;
        Ok(VcsTagRow {
            name: name.to_string(),
            branch: branch.to_string(),
            doc: doc.to_string(),
            seq,
            created_at_ms: now_ms,
        })
    }

    pub fn vcs_tag_list(&mut self, workspace: &WorkspaceId) -> Result<Vec<VcsTagRow>, StoreError> {
        let tx = self.conn.transaction()?;
        let tags = {
            let mut stmt = tx.prepare(
                "SELECT name, branch, doc, seq, created_at_ms FROM vcs_tags \
                 WHERE workspace=?1 ORDER BY created_at_ms DESC, name ASC",
            )?;
            let mut rows = stmt.query(params![workspace.as_str()])?;
            let mut tags = Vec::new();
            while let Some(row) = rows.next()? {
                tags.push(VcsTagRow {
                    name: row.get(0)?,
                    branch: row.get(1)?,
                    doc: row.get(2)?,
                    seq: row.get(3)?,
                    created_at_ms: row.get(4)?,
                });
            }
            tags
        };
        tx.commit()?;
        Ok(tags)
    }

    pub fn vcs_tag_delete(
        &mut self,
        workspace: &WorkspaceId,
        name: &str,
    ) -> Result<bool, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput("name must not be empty"));
        }
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM vcs_tags WHERE workspace=?1 AND name=?2",
            params![workspace.as_str(), name],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }
}
