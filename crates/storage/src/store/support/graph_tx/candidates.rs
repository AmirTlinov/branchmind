#![forbid(unsafe_code)]

use super::super::super::{BranchSource, GraphEdgeRow, GraphNodeRow, StoreError};
use super::query::{graph_edges_tail_tx, graph_nodes_tail_tx};
use super::tags::decode_tags;
use super::types::{GraphDiffCandidate, GraphEdgeKey, GraphMergeCandidate};
use rusqlite::{Transaction, params};

pub(in crate::store) fn graph_diff_candidates_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    sources: &[BranchSource],
    doc: &str,
    before_seq: i64,
    limit: i64,
) -> Result<Vec<GraphDiffCandidate>, StoreError> {
    let nodes = graph_nodes_tail_tx(tx, workspace, sources, doc, before_seq, limit, true)?;
    let edges = graph_edges_tail_tx(tx, workspace, sources, doc, before_seq, limit, true)?;

    let mut out = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while out.len() < limit as usize && (i < nodes.len() || j < edges.len()) {
        let take_node = match (nodes.get(i), edges.get(j)) {
            (Some(n), Some(e)) => n.last_seq >= e.last_seq,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => false,
        };

        if take_node {
            let node = nodes[i].clone();
            out.push(GraphDiffCandidate::Node { to: node });
            i += 1;
        } else {
            let edge = edges[j].clone();
            out.push(GraphDiffCandidate::Edge {
                key: GraphEdgeKey {
                    from: edge.from.clone(),
                    rel: edge.rel.clone(),
                    to: edge.to.clone(),
                },
                to: edge,
            });
            j += 1;
        }
    }
    Ok(out)
}

pub(in crate::store) fn graph_merge_candidates_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    from_branch: &str,
    doc: &str,
    base_cutoff_seq: i64,
    before_seq: i64,
    limit: i64,
) -> Result<Vec<GraphMergeCandidate>, StoreError> {
    let limit = limit.clamp(1, 1000);

    let mut node_stmt = tx.prepare(
        r#"
        WITH latest AS (
          SELECT node_id, MAX(seq) AS max_seq
          FROM graph_node_versions
          WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq > ?4 AND seq < ?5
          GROUP BY node_id
        )
        SELECT v.node_id, v.node_type, v.title, v.text, v.tags, v.status, v.meta_json, v.deleted, v.seq, v.ts_ms
        FROM graph_node_versions v
        JOIN latest l ON v.node_id=l.node_id AND v.seq=l.max_seq
        ORDER BY v.seq DESC
        LIMIT ?6
        "#,
    )?;
    let mut node_rows = node_stmt.query(params![
        workspace,
        from_branch,
        doc,
        base_cutoff_seq,
        before_seq,
        limit
    ])?;
    let mut nodes = Vec::new();
    while let Some(row) = node_rows.next()? {
        let raw_tags: Option<String> = row.get(4)?;
        let deleted: i64 = row.get(7)?;
        nodes.push(GraphNodeRow {
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

    let mut edge_stmt = tx.prepare(
        r#"
        WITH latest AS (
          SELECT from_id, rel, to_id, MAX(seq) AS max_seq
          FROM graph_edge_versions
          WHERE workspace=?1 AND branch=?2 AND doc=?3 AND seq > ?4 AND seq < ?5
          GROUP BY from_id, rel, to_id
        )
        SELECT v.from_id, v.rel, v.to_id, v.meta_json, v.deleted, v.seq, v.ts_ms
        FROM graph_edge_versions v
        JOIN latest l ON v.from_id=l.from_id AND v.rel=l.rel AND v.to_id=l.to_id AND v.seq=l.max_seq
        ORDER BY v.seq DESC
        LIMIT ?6
        "#,
    )?;
    let mut edge_rows = edge_stmt.query(params![
        workspace,
        from_branch,
        doc,
        base_cutoff_seq,
        before_seq,
        limit
    ])?;
    let mut edges = Vec::new();
    while let Some(row) = edge_rows.next()? {
        let deleted: i64 = row.get(4)?;
        edges.push(GraphEdgeRow {
            from: row.get(0)?,
            rel: row.get(1)?,
            to: row.get(2)?,
            meta_json: row.get(3)?,
            deleted: deleted != 0,
            last_seq: row.get(5)?,
            last_ts_ms: row.get(6)?,
        });
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while out.len() < limit as usize && (i < nodes.len() || j < edges.len()) {
        let take_node = match (nodes.get(i), edges.get(j)) {
            (Some(n), Some(e)) => n.last_seq >= e.last_seq,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => false,
        };

        if take_node {
            out.push(GraphMergeCandidate::Node {
                theirs: nodes[i].clone(),
            });
            i += 1;
        } else {
            out.push(GraphMergeCandidate::Edge {
                theirs: edges[j].clone(),
            });
            j += 1;
        }
    }
    Ok(out)
}
