#![forbid(unsafe_code)]

use super::super::super::append_sources_clause;
use super::super::types::GraphEdgeKey;
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, params_from_iter};
use std::collections::HashMap;

use super::super::super::super::{BranchSource, GraphEdgeRow, StoreError};

pub(in crate::store) fn graph_edges_tail_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
    include_deleted: bool,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    let limit = limit.clamp(1, 2000);
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND seq < ? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    params.push(SqlValue::Integer(before_seq));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
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
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}

pub(in crate::store) fn graph_edges_all_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    include_deleted: bool,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(
        "), latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }
    sql.push_str(" ORDER BY c.seq DESC");

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}

pub(in crate::store) fn graph_edge_get_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    key: &GraphEdgeKey,
) -> Result<Option<GraphEdgeRow>, StoreError> {
    let mut sql = String::from(
        "SELECT meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND from_id=? AND rel=? AND to_id=? AND ",
    );
    let mut params: Vec<SqlValue> = vec![
        SqlValue::Text(workspace.to_string()),
        SqlValue::Text(doc.to_string()),
        SqlValue::Text(key.from.clone()),
        SqlValue::Text(key.rel.clone()),
        SqlValue::Text(key.to.clone()),
    ];
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" ORDER BY seq DESC LIMIT 1");

    let mut stmt = tx.prepare(&sql)?;
    let row = stmt
        .query_row(params_from_iter(params.iter()), |row| {
            let deleted: i64 = row.get(1)?;
            Ok(GraphEdgeRow {
                from: key.from.clone(),
                rel: key.rel.clone(),
                to: key.to.clone(),
                meta_json: row.get(0)?,
                deleted: deleted != 0,
                last_seq: row.get(2)?,
                last_ts_ms: row.get(3)?,
            })
        })
        .optional()?;
    Ok(row)
}

pub(in crate::store) fn graph_edges_get_map_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    edge_keys: &[GraphEdgeKey],
    include_deleted: bool,
) -> Result<HashMap<GraphEdgeKey, GraphEdgeRow>, StoreError> {
    if edge_keys.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND (");
    for (i, key) in edge_keys.iter().enumerate() {
        if i != 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(from_id=? AND rel=? AND to_id=?)");
        params.push(SqlValue::Text(key.from.clone()));
        params.push(SqlValue::Text(key.rel.clone()));
        params.push(SqlValue::Text(key.to.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq",
    );
    if !include_deleted {
        sql.push_str(" WHERE c.deleted=0");
    }

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        let from: String = row.get(0)?;
        let rel: String = row.get(1)?;
        let to: String = row.get(2)?;
        let deleted: i64 = row.get(4)?;
        let key = GraphEdgeKey {
            from: from.clone(),
            rel: rel.clone(),
            to: to.clone(),
        };
        out.insert(
            key,
            GraphEdgeRow {
                from,
                rel,
                to,
                meta_json: row.get(3)?,
                deleted: deleted != 0,
                last_seq: row.get(5)?,
                last_ts_ms: row.get(6)?,
            },
        );
    }
    Ok(out)
}

pub(in crate::store) fn graph_edge_keys_for_node_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_id: &str,
) -> Result<Vec<GraphEdgeKey>, StoreError> {
    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, deleted, seq \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND (from_id=? OR to_id=?))");
    params.push(SqlValue::Text(node_id.to_string()));
    params.push(SqlValue::Text(node_id.to_string()));
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.deleted \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq \
         WHERE c.deleted=0",
    );

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(3)?;
        if deleted != 0 {
            continue;
        }
        out.push(GraphEdgeKey {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
        });
    }
    Ok(out)
}

pub(in crate::store) fn graph_edges_for_nodes_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    node_ids: &[String],
    limit: i64,
) -> Result<Vec<GraphEdgeRow>, StoreError> {
    if node_ids.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 5000);

    let mut sql = String::from(
        "WITH candidates AS (SELECT from_id, rel, to_id, meta_json, deleted, seq, ts_ms \
         FROM graph_edge_versions WHERE workspace=? AND doc=? AND ",
    );
    let mut params: Vec<SqlValue> = Vec::new();
    params.push(SqlValue::Text(workspace.to_string()));
    params.push(SqlValue::Text(doc.to_string()));
    append_sources_clause(&mut sql, &mut params, sources);
    sql.push_str(" AND from_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str(") AND to_id IN (");
    for (i, id) in node_ids.iter().enumerate() {
        if i != 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(id.clone()));
    }
    sql.push_str("))");
    sql.push_str(
        ", latest AS (SELECT from_id, rel, to_id, MAX(seq) AS max_seq FROM candidates GROUP BY from_id, rel, to_id) \
         SELECT c.from_id, c.rel, c.to_id, c.meta_json, c.deleted, c.seq, c.ts_ms \
         FROM candidates c JOIN latest l ON c.from_id=l.from_id AND c.rel=l.rel AND c.to_id=l.to_id AND c.seq=l.max_seq \
         ORDER BY c.seq DESC LIMIT ?",
    );
    params.push(SqlValue::Integer(limit));

    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let deleted: i64 = row.get(4)?;
        out.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }
    Ok(out)
}
