use super::super::super::{BranchSource, StoreError};
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};

pub(in crate::store) fn doc_entries_head_seq_tx(
    tx: &Transaction<'_>,
    workspace: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT seq FROM doc_entries WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
            params![workspace],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

pub(in crate::store) fn doc_head_seq_for_sources_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    doc: &str,
    sources: &[BranchSource],
) -> Result<Option<i64>, StoreError> {
    let mut sql =
        String::from("SELECT MAX(seq) FROM doc_entries WHERE workspace=?1 AND doc=?2 AND (");
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));

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
    sql.push(')');

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;
    if let Some(row) = rows.next()? {
        Ok(row.get::<_, Option<i64>>(0)?)
    } else {
        Ok(None)
    }
}
