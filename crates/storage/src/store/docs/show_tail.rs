#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;

impl SqliteStore {
    pub fn doc_show_tail(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<DocSlice, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let tx = self.conn.transaction()?;

        let mut entries_desc = Vec::new();
        {
            let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;

            let mut sql = String::from(
                "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
                 FROM doc_entries \
                 WHERE workspace=? AND doc=? AND seq < ? AND (",
            );
            let mut params: Vec<SqlValue> = Vec::new();
            params.push(SqlValue::Text(workspace.as_str().to_string()));
            params.push(SqlValue::Text(doc.to_string()));
            params.push(SqlValue::Integer(before_seq));

            for (index, source) in sources.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("(branch=?");
                params.push(SqlValue::Text(source.branch.clone()));
                if let Some(cutoff) = source.cutoff_seq {
                    sql.push_str(" AND seq <= ?");
                    params.push(SqlValue::Integer(cutoff));
                }
                sql.push(')');
            }

            sql.push_str(") ORDER BY seq DESC LIMIT ?");
            params.push(SqlValue::Integer(limit + 1));

            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(params))?;

            while let Some(row) = rows.next()? {
                let kind_str: String = row.get(3)?;
                let kind = match kind_str.as_str() {
                    "note" => DocEntryKind::Note,
                    "event" => DocEntryKind::Event,
                    _ => DocEntryKind::Event,
                };
                entries_desc.push(DocEntryRow {
                    seq: row.get(0)?,
                    ts_ms: row.get(1)?,
                    branch: row.get(2)?,
                    doc: doc.to_string(),
                    kind,
                    title: row.get(4)?,
                    format: row.get(5)?,
                    meta_json: row.get(6)?,
                    content: row.get(7)?,
                    source_event_id: row.get(8)?,
                    event_type: row.get(9)?,
                    task_id: row.get(10)?,
                    path: row.get(11)?,
                    payload_json: row.get(12)?,
                });
            }
        }

        let has_more = entries_desc.len() as i64 > limit;
        if has_more {
            entries_desc.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            entries_desc.last().map(|e| e.seq)
        } else {
            None
        };

        entries_desc.reverse();
        tx.commit()?;

        Ok(DocSlice {
            entries: entries_desc,
            next_cursor,
            has_more,
        })
    }
}
