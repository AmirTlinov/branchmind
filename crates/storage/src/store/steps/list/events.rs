#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn list_events_for_task(
        &self,
        workspace: &WorkspaceId,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, type, payload_json
            FROM events
            WHERE workspace = ?1 AND task_id = ?2
            ORDER BY seq DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), task_id, limit as i64], |row| {
            Ok(EventRow {
                seq: row.get(0)?,
                ts_ms: row.get(1)?,
                task_id: row.get(2)?,
                path: row.get(3)?,
                event_type: row.get(4)?,
                payload_json: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_events(
        &self,
        workspace: &WorkspaceId,
        since_event_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventRow>, StoreError> {
        let since_seq = match since_event_id {
            None => 0i64,
            Some(event_id) => parse_event_id(event_id).ok_or(StoreError::InvalidInput(
                "since must be like evt_<16-digit-seq>",
            ))?,
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT seq, ts_ms, task_id, path, type, payload_json
            FROM events
            WHERE workspace = ?1 AND seq > ?2
            ORDER BY seq ASC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(
            params![workspace.as_str(), since_seq, limit as i64],
            |row| {
                Ok(EventRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    task_id: row.get(2)?,
                    path: row.get(3)?,
                    event_type: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
