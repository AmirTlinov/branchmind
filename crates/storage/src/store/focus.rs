#![forbid(unsafe_code)]

use super::{SqliteStore, StoreError};
use bm_core::ids::WorkspaceId;
use rusqlite::OptionalExtension;
use rusqlite::params;

impl SqliteStore {
    pub fn focus_set(&mut self, workspace: &WorkspaceId, focus_id: &str) -> Result<(), StoreError> {
        let now_ms = super::now_ms();
        let tx = self.conn.transaction()?;
        super::ensure_workspace_tx(&tx, workspace, now_ms)?;
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
}
