use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
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
}
