use super::super::super::{DocEntryKind, DocEntryRow, DocSlice, StoreError};
use super::branches::branch_sources_tx;
use super::sources_clause::append_sources_clause;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Transaction, params_from_iter};

pub(in crate::store) fn doc_diff_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    to_branch: &str,
    doc: &str,
    before_seq: i64,
    limit: i64,
) -> Result<DocSlice, StoreError> {
    let from_sources = branch_sources_tx(tx, workspace, from_branch)?;
    let to_sources = branch_sources_tx(tx, workspace, to_branch)?;

    let mut sql = String::from(
        "SELECT seq, ts_ms, branch, kind, title, format, meta_json, content, source_event_id, event_type, task_id, path, payload_json \
         FROM doc_entries \
         WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));

    append_sources_clause(&mut sql, &mut params, &to_sources);
    sql.push_str(" AND NOT ");
    append_sources_clause(&mut sql, &mut params, &from_sources);
    sql.push_str(" ORDER BY seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit + 1));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;

    let mut entries_desc = Vec::new();
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

    Ok(DocSlice {
        entries: entries_desc,
        next_cursor,
        has_more,
    })
}
