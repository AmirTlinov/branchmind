#![forbid(unsafe_code)]

use super::super::super::append_sources_clause;
use super::super::tags::{decode_tags, normalize_tags};
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, params_from_iter};
use std::collections::HashMap;

use super::super::super::super::{BranchSource, GraphNodeRow, GraphQueryRequest, StoreError};

pub(in crate::store) fn graph_nodes_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    include_deleted: bool,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let limit = limit.clamp(1, 1000);
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}

pub(in crate::store) fn graph_nodes_all_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    include_deleted: bool,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC");

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}

pub(in crate::store) fn graph_node_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_id: &str,
) -> Result<Option<GraphNodeRow>, StoreError> {
    let mut sql = String::from(
        "SELECT node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND node_id=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Text(node_id.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" ORDER BY seq DESC LIMIT 1");

    let mut stmt = tx.prepare(&sql)?;
    let row = stmt
        .query_row(params_from_iter(params.iter()), |row| {
            let raw_tags: Option<String> = row.get(3)?;
            let deleted: i64 = row.get(6)?;
            Ok(GraphNodeRow {
                id: node_id.to_string(),
                node_type: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                title: row.get(1)?,
                text: row.get(2)?,
                tags: decode_tags(raw_tags.as_deref()),
                status: row.get(4)?,
                meta_json: row.get(5)?,
                deleted: deleted != 0,
                last_seq: row.get(7)?,
                last_ts_ms: row.get(8)?,
            })
        })
        .optional()?;
    Ok(row)
}

pub(in crate::store) fn graph_nodes_get_map_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_ids: &[String],
    include_deleted: bool,
) -> Result<HashMap<String, GraphNodeRow>, StoreError> {
    if node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND node_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.insert(
            id.clone(),
            GraphNodeRow {
                id,
                node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                title: row.get(2)?,
                text: row.get(3)?,
                tags: decode_tags(raw_tags.as_deref()),
                status: row.get(5)?,
                meta_json: row.get(6)?,
                deleted: deleted != 0,
                last_seq: row.get(8)?,
                last_ts_ms: row.get(9)?,
            },
        );
    }
    Ok(out)
}

pub(in crate::store) fn graph_nodes_query_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    request: &GraphQueryRequest,
) -> Result<Vec<GraphNodeRow>, StoreError> {
    let limit = limit.clamp(1, 200);
    let mut sql = String::from(
        "WITH candidates AS (SELECT node_id, node_type, title, text, tags, status, meta_json, deleted, seq, ts_ms \
         FROM graph_node_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT node_id, MAX(seq) AS max_seq FROM candidates GROUP BY node_id) \
         SELECT c.node_id, c.node_type, c.title, c.text, c.tags, c.status, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.node_id=l.node_id AND c.seq=l.max_seq WHERE 1=1",
    );

    if let Some(ids) = request.ids.as_ref().filter(|v| !v.is_empty()) {
        sql.push_str(" AND c.node_id IN (");
        for (i, id) in ids.iter().enumerate() {
            if i != 0 {
                sql.push(',');
            }
            sql.push('?');
            params.push(SqlValue::Text(id.clone()));
        }
        sql.push(')');
    }

    if let Some(types) = request.types.as_ref().filter(|v| !v.is_empty()) {
        sql.push_str(" AND c.node_type IN (");
        for (i, ty) in types.iter().enumerate() {
            if i != 0 {
                sql.push(',');
            }
            sql.push('?');
            params.push(SqlValue::Text(ty.clone()));
        }
        sql.push(')');
    }

    if let Some(status) = request
        .status
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        sql.push_str(" AND c.status=?");
        params.push(SqlValue::Text(status.to_string()));
    }

    if let Some(tags_any) = request.tags_any.as_ref().filter(|v| !v.is_empty()) {
        let tags_any = normalize_tags(tags_any)?;
        if !tags_any.is_empty() {
            sql.push_str(" AND (");
            for (i, tag) in tags_any.iter().enumerate() {
                if i != 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("COALESCE(c.tags,'') LIKE ?");
                params.push(SqlValue::Text(format!("%\n{}\n%", tag)));
            }
            sql.push(')');
        }
    }

    if let Some(tags_all) = request.tags_all.as_ref().filter(|v| !v.is_empty()) {
        let tags_all = normalize_tags(tags_all)?;
        for tag in tags_all {
            sql.push_str(" AND COALESCE(c.tags,'') LIKE ?");
            params.push(SqlValue::Text(format!("%\n{}\n%", tag)));
        }
    }

    if let Some(text) = request
        .text
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        sql.push_str(
            " AND instr(lower(COALESCE(c.title,'') || '\n' || COALESCE(c.text,'')), lower(?)) > 0",
        );
        params.push(SqlValue::Text(text.to_string()));
    }

    sql.push_str(" ORDER BY c.seq DESC LIMIT ?");
    params.push(SqlValue::Integer(limit + 1));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        out.push(GraphNodeRow {
            id: row.get(0)?,
            node_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            title: row.get(2)?,
            text: row.get(3)?,
            tags: decode_tags(raw_tags.as_deref()),
            status: row.get(5)?,
            meta_json: row.get(6)?,
            deleted: deleted != 0,
            last_seq: row.get(8)?,
            last_ts_ms: row.get(9)?,
        });
    }
    Ok(out)
}
