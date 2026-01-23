#![forbid(unsafe_code)]

use super::{SqliteStore, StoreError};
use bm_core::ids::WorkspaceId;
use rusqlite::OptionalExtension;
use rusqlite::params;

impl SqliteStore {
    pub fn portal_cursor_get(
        &self,
        workspace: &WorkspaceId,
        tool: &str,
        target_id: &str,
        lane: &str,
    ) -> Result<Option<i64>, StoreError> {
        let tool = tool.trim();
        let target_id = target_id.trim();
        let lane = lane.trim();
        if tool.is_empty() {
            return Err(StoreError::InvalidInput("tool must not be empty"));
        }
        if target_id.is_empty() {
            return Err(StoreError::InvalidInput("target_id must not be empty"));
        }
        if lane.is_empty() {
            return Err(StoreError::InvalidInput("lane must not be empty"));
        }

        Ok(self
            .conn
            .query_row(
                r#"
                SELECT last_seq
                FROM portal_cursors
                WHERE workspace=?1 AND tool=?2 AND target_id=?3 AND lane=?4
                "#,
                params![workspace.as_str(), tool, target_id, lane],
                |row| row.get::<_, i64>(0),
            )
            .optional()?)
    }

    pub fn portal_cursor_set(
        &mut self,
        workspace: &WorkspaceId,
        tool: &str,
        target_id: &str,
        lane: &str,
        last_seq: i64,
    ) -> Result<(), StoreError> {
        let tool = tool.trim();
        let target_id = target_id.trim();
        let lane = lane.trim();
        if tool.is_empty() {
            return Err(StoreError::InvalidInput("tool must not be empty"));
        }
        if target_id.is_empty() {
            return Err(StoreError::InvalidInput("target_id must not be empty"));
        }
        if lane.is_empty() {
            return Err(StoreError::InvalidInput("lane must not be empty"));
        }
        if last_seq < 0 {
            return Err(StoreError::InvalidInput("last_seq must be >= 0"));
        }

        let now_ms = super::now_ms();
        let tx = self.conn.transaction()?;
        super::ensure_workspace_tx(&tx, workspace, now_ms)?;
        tx.execute(
            r#"
            INSERT INTO portal_cursors(workspace, tool, target_id, lane, last_seq, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(workspace, tool, target_id, lane) DO UPDATE SET
              last_seq=excluded.last_seq,
              updated_at_ms=excluded.updated_at_ms
            "#,
            params![workspace.as_str(), tool, target_id, lane, last_seq, now_ms],
        )?;
        tx.commit()?;
        Ok(())
    }
}
