#![forbid(unsafe_code)]

use crate::store::{GraphConflictDetail, GraphEdgeRow, GraphNodeRow, StoreError};

use super::super::tags::decode_tags;
use rusqlite::{OptionalExtension, Transaction, params};

#[derive(Clone, Debug)]
pub(in crate::store) struct GraphConflictDetailRow {
    kind: String,
    key: String,
    from_branch: String,
    into_branch: String,
    doc: String,
    status: String,
    created_at_ms: i64,
    resolved_at_ms: Option<i64>,

    base_seq: Option<i64>,
    base_ts_ms: Option<i64>,
    base_deleted: Option<i64>,
    base_node_type: Option<String>,
    base_title: Option<String>,
    base_text: Option<String>,
    base_tags: Option<String>,
    base_status: Option<String>,
    base_meta_json: Option<String>,
    base_from_id: Option<String>,
    base_rel: Option<String>,
    base_to_id: Option<String>,
    base_edge_meta_json: Option<String>,

    theirs_seq: Option<i64>,
    theirs_ts_ms: Option<i64>,
    theirs_deleted: Option<i64>,
    theirs_node_type: Option<String>,
    theirs_title: Option<String>,
    theirs_text: Option<String>,
    theirs_tags: Option<String>,
    theirs_status: Option<String>,
    theirs_meta_json: Option<String>,
    theirs_from_id: Option<String>,
    theirs_rel: Option<String>,
    theirs_to_id: Option<String>,
    theirs_edge_meta_json: Option<String>,

    ours_seq: Option<i64>,
    ours_ts_ms: Option<i64>,
    ours_deleted: Option<i64>,
    ours_node_type: Option<String>,
    ours_title: Option<String>,
    ours_text: Option<String>,
    ours_tags: Option<String>,
    ours_status: Option<String>,
    ours_meta_json: Option<String>,
    ours_from_id: Option<String>,
    ours_rel: Option<String>,
    ours_to_id: Option<String>,
    ours_edge_meta_json: Option<String>,
}

impl GraphConflictDetailRow {
    pub(in crate::store) fn into_detail(self, conflict_id: &str) -> GraphConflictDetail {
        let kind = self.kind.clone();
        let key = self.key.clone();

        let base_node = if kind == "node" && self.base_seq.is_some() {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.base_node_type.unwrap_or_default(),
                title: self.base_title,
                text: self.base_text,
                tags: decode_tags(self.base_tags.as_deref()),
                status: self.base_status,
                meta_json: self.base_meta_json,
                deleted: self.base_deleted.unwrap_or(0) != 0,
                last_seq: self.base_seq.unwrap_or(0),
                last_ts_ms: self.base_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let theirs_node = if kind == "node" && self.theirs_seq.unwrap_or(0) != 0 {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.theirs_node_type.unwrap_or_default(),
                title: self.theirs_title,
                text: self.theirs_text,
                tags: decode_tags(self.theirs_tags.as_deref()),
                status: self.theirs_status,
                meta_json: self.theirs_meta_json,
                deleted: self.theirs_deleted.unwrap_or(0) != 0,
                last_seq: self.theirs_seq.unwrap_or(0),
                last_ts_ms: self.theirs_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let ours_node = if kind == "node" && self.ours_seq.unwrap_or(0) != 0 {
            Some(GraphNodeRow {
                id: key.clone(),
                node_type: self.ours_node_type.unwrap_or_default(),
                title: self.ours_title,
                text: self.ours_text,
                tags: decode_tags(self.ours_tags.as_deref()),
                status: self.ours_status,
                meta_json: self.ours_meta_json,
                deleted: self.ours_deleted.unwrap_or(0) != 0,
                last_seq: self.ours_seq.unwrap_or(0),
                last_ts_ms: self.ours_ts_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let base_edge = if kind == "edge" && self.base_seq.is_some() {
            match (self.base_from_id, self.base_rel, self.base_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.base_edge_meta_json,
                    deleted: self.base_deleted.unwrap_or(0) != 0,
                    last_seq: self.base_seq.unwrap_or(0),
                    last_ts_ms: self.base_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        let theirs_edge = if kind == "edge" && self.theirs_seq.unwrap_or(0) != 0 {
            match (self.theirs_from_id, self.theirs_rel, self.theirs_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.theirs_edge_meta_json,
                    deleted: self.theirs_deleted.unwrap_or(0) != 0,
                    last_seq: self.theirs_seq.unwrap_or(0),
                    last_ts_ms: self.theirs_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        let ours_edge = if kind == "edge" && self.ours_seq.unwrap_or(0) != 0 {
            match (self.ours_from_id, self.ours_rel, self.ours_to_id) {
                (Some(from), Some(rel), Some(to)) => Some(GraphEdgeRow {
                    from,
                    rel,
                    to,
                    meta_json: self.ours_edge_meta_json,
                    deleted: self.ours_deleted.unwrap_or(0) != 0,
                    last_seq: self.ours_seq.unwrap_or(0),
                    last_ts_ms: self.ours_ts_ms.unwrap_or(0),
                }),
                _ => None,
            }
        } else {
            None
        };

        GraphConflictDetail {
            conflict_id: conflict_id.to_string(),
            kind,
            key,
            from_branch: self.from_branch,
            into_branch: self.into_branch,
            doc: self.doc,
            status: self.status,
            created_at_ms: self.created_at_ms,
            resolved_at_ms: self.resolved_at_ms,
            base_node,
            theirs_node,
            ours_node,
            base_edge,
            theirs_edge,
            ours_edge,
        }
    }
}

pub(in crate::store) fn graph_conflict_detail_row_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    conflict_id: &str,
) -> Result<Option<GraphConflictDetailRow>, StoreError> {
    Ok(tx
        .query_row(
            r#"
            SELECT kind, key, from_branch, into_branch, doc, status, created_at_ms, resolved_at_ms,
                   base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
                   base_from_id, base_rel, base_to_id, base_edge_meta_json,
                   theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
                   theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
                   ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
                   ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json
            FROM graph_conflicts
            WHERE workspace=?1 AND conflict_id=?2
            "#,
            params![workspace, conflict_id],
            |row| {
                Ok(GraphConflictDetailRow {
                    kind: row.get(0)?,
                    key: row.get(1)?,
                    from_branch: row.get(2)?,
                    into_branch: row.get(3)?,
                    doc: row.get(4)?,
                    status: row.get(5)?,
                    created_at_ms: row.get(6)?,
                    resolved_at_ms: row.get(7)?,
                    base_seq: row.get(8)?,
                    base_ts_ms: row.get(9)?,
                    base_deleted: row.get(10)?,
                    base_node_type: row.get(11)?,
                    base_title: row.get(12)?,
                    base_text: row.get(13)?,
                    base_tags: row.get(14)?,
                    base_status: row.get(15)?,
                    base_meta_json: row.get(16)?,
                    base_from_id: row.get(17)?,
                    base_rel: row.get(18)?,
                    base_to_id: row.get(19)?,
                    base_edge_meta_json: row.get(20)?,
                    theirs_seq: row.get(21)?,
                    theirs_ts_ms: row.get(22)?,
                    theirs_deleted: row.get(23)?,
                    theirs_node_type: row.get(24)?,
                    theirs_title: row.get(25)?,
                    theirs_text: row.get(26)?,
                    theirs_tags: row.get(27)?,
                    theirs_status: row.get(28)?,
                    theirs_meta_json: row.get(29)?,
                    theirs_from_id: row.get(30)?,
                    theirs_rel: row.get(31)?,
                    theirs_to_id: row.get(32)?,
                    theirs_edge_meta_json: row.get(33)?,
                    ours_seq: row.get(34)?,
                    ours_ts_ms: row.get(35)?,
                    ours_deleted: row.get(36)?,
                    ours_node_type: row.get(37)?,
                    ours_title: row.get(38)?,
                    ours_text: row.get(39)?,
                    ours_tags: row.get(40)?,
                    ours_status: row.get(41)?,
                    ours_meta_json: row.get(42)?,
                    ours_from_id: row.get(43)?,
                    ours_rel: row.get(44)?,
                    ours_to_id: row.get(45)?,
                    ours_edge_meta_json: row.get(46)?,
                })
            },
        )
        .optional()?)
}
