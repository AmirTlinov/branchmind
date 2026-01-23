#![forbid(unsafe_code)]

use super::super::{SqliteStore, StoreError};
use bm_core::ids::WorkspaceId;
use rusqlite::OptionalExtension;
use rusqlite::params;

impl SqliteStore {
    pub fn doc_entry_get_by_seq(
        &self,
        workspace: &WorkspaceId,
        seq: i64,
    ) -> Result<Option<super::super::DocEntryRow>, StoreError> {
        if seq < 0 {
            return Err(StoreError::InvalidInput("seq must be >= 0"));
        }

        Ok(self
            .conn
            .query_row(
                r#"
                SELECT
                  seq, ts_ms, branch, doc, kind,
                  title, format, meta_json, content,
                  source_event_id, event_type, task_id, path, payload_json
                FROM doc_entries
                WHERE workspace=?1 AND seq=?2
                "#,
                params![workspace.as_str(), seq],
                |row| {
                    let kind_raw: String = row.get(4)?;
                    let kind = match kind_raw.as_str() {
                        "note" => super::super::DocEntryKind::Note,
                        "event" => super::super::DocEntryKind::Event,
                        _ => super::super::DocEntryKind::Event,
                    };
                    Ok(super::super::DocEntryRow {
                        seq: row.get(0)?,
                        ts_ms: row.get(1)?,
                        branch: row.get(2)?,
                        doc: row.get(3)?,
                        kind,
                        title: row.get(5)?,
                        format: row.get(6)?,
                        meta_json: row.get(7)?,
                        content: row.get(8)?,
                        source_event_id: row.get(9)?,
                        event_type: row.get(10)?,
                        task_id: row.get(11)?,
                        path: row.get(12)?,
                        payload_json: row.get(13)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn doc_entries_since(
        &self,
        workspace: &WorkspaceId,
        request: super::super::DocEntriesSinceRequest,
    ) -> Result<super::super::DocEntriesSinceResult, StoreError> {
        let super::super::DocEntriesSinceRequest {
            branch,
            doc,
            since_seq,
            limit,
            kind,
        } = request;

        let branch = branch.trim().to_string();
        let doc = doc.trim().to_string();
        if branch.is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if since_seq < 0 {
            return Err(StoreError::InvalidInput("since_seq must be >= 0"));
        }

        let limit = limit.clamp(0, 500) as i64;
        if limit == 0 {
            return Ok(super::super::DocEntriesSinceResult {
                entries: Vec::new(),
                total: 0,
            });
        }

        let kind_str = kind.map(|k| k.as_str().to_string());

        let mut entries = Vec::new();
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              seq, ts_ms, branch, doc, kind,
              title, format, meta_json, content,
              source_event_id, event_type, task_id, path, payload_json
            FROM doc_entries
            WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq>?4
              AND (?5 IS NULL OR kind=?5)
            ORDER BY seq ASC
            LIMIT ?6
            "#,
        )?;
        let mut rows = stmt.query(params![
            workspace.as_str(),
            branch,
            doc,
            since_seq,
            kind_str,
            limit
        ])?;

        while let Some(row) = rows.next()? {
            let kind_raw: String = row.get(4)?;
            let kind = match kind_raw.as_str() {
                "note" => super::super::DocEntryKind::Note,
                "event" => super::super::DocEntryKind::Event,
                _ => super::super::DocEntryKind::Event,
            };
            entries.push(super::super::DocEntryRow {
                seq: row.get(0)?,
                ts_ms: row.get(1)?,
                branch: row.get(2)?,
                doc: row.get(3)?,
                kind,
                title: row.get(5)?,
                format: row.get(6)?,
                meta_json: row.get(7)?,
                content: row.get(8)?,
                source_event_id: row.get(9)?,
                event_type: row.get(10)?,
                task_id: row.get(11)?,
                path: row.get(12)?,
                payload_json: row.get(13)?,
            });
        }

        let total: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(1)
            FROM doc_entries
            WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq>?4
              AND (?5 IS NULL OR kind=?5)
            "#,
            params![workspace.as_str(), branch, doc, since_seq, kind_str],
            |row| row.get::<_, i64>(0),
        )?;

        Ok(super::super::DocEntriesSinceResult {
            entries,
            total: total.max(0) as usize,
        })
    }
}
