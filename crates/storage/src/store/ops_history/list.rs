#![forbid(unsafe_code)]

use super::super::{OpsHistoryRow, SqliteStore, StoreError};
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn ops_history_list(
        &self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OpsHistoryRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, intent, payload_json, before_json, after_json, undoable, undone
            FROM ops_history
            WHERE workspace=?1 AND (?2 IS NULL OR task_id=?2)
            ORDER BY seq DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), task_id, limit as i64], |row| {
            Ok(OpsHistoryRow {
                seq: row.get(0)?,
                ts_ms: row.get(1)?,
                task_id: row.get(2)?,
                path: row.get(3)?,
                intent: row.get(4)?,
                payload_json: row.get(5)?,
                before_json: row.get(6)?,
                after_json: row.get(7)?,
                undoable: row.get::<_, i64>(8)? != 0,
                undone: row.get::<_, i64>(9)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
