#![forbid(unsafe_code)]

use crate::store::{GraphEdgeRow, GraphNodeRow, StoreError};

use super::super::tags::encode_tags;
use super::super::types::{
    GraphConflictCreateCtx, GraphConflictIdArgs, GraphConflictSignatureArgs, GraphEdgeKey,
};
use super::id::graph_conflict_id;
use super::status_row::graph_conflict_status_row_by_signature_tx;
use rusqlite::{Transaction, params};

#[derive(Clone, Debug)]
pub(in crate::store) struct GraphConflictCreateResult {
    pub(in crate::store) inserted: bool,
}

pub(in crate::store) fn graph_conflict_create_node_tx(
    tx: &Transaction<'_>,
    ctx: &GraphConflictCreateCtx<'_>,
    key: &str,
    base: Option<&GraphNodeRow>,
    theirs: Option<&GraphNodeRow>,
    ours: Option<&GraphNodeRow>,
) -> Result<GraphConflictCreateResult, StoreError> {
    let theirs_seq = theirs.map(|n| n.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|n| n.last_seq).unwrap_or(0);

    if let Some(existing) = graph_conflict_status_row_by_signature_tx(
        tx,
        GraphConflictSignatureArgs {
            workspace: ctx.workspace,
            from_branch: ctx.from_branch,
            into_branch: ctx.into_branch,
            doc: ctx.doc,
            kind: "node",
            key,
            base_cutoff_seq: ctx.base_cutoff_seq,
            theirs_seq,
        },
    )? {
        // Keep snapshots fresh for non-resolved conflicts, but never override resolution state.
        if existing.status != "resolved" {
            let base_tags = base.and_then(|n| encode_tags(&n.tags));
            let theirs_tags = theirs.and_then(|n| encode_tags(&n.tags));
            let ours_tags = ours.and_then(|n| encode_tags(&n.tags));
            tx.execute(
                r#"
                UPDATE graph_conflicts
                SET
                  base_seq=?3,
                  base_ts_ms=?4,
                  base_deleted=?5,
                  base_node_type=?6,
                  base_title=?7,
                  base_text=?8,
                  base_tags=?9,
                  base_status=?10,
                  base_meta_json=?11,

                  theirs_seq=?12,
                  theirs_ts_ms=?13,
                  theirs_deleted=?14,
                  theirs_node_type=?15,
                  theirs_title=?16,
                  theirs_text=?17,
                  theirs_tags=?18,
                  theirs_status=?19,
                  theirs_meta_json=?20,

                  ours_seq=?21,
                  ours_ts_ms=?22,
                  ours_deleted=?23,
                  ours_node_type=?24,
                  ours_title=?25,
                  ours_text=?26,
                  ours_tags=?27,
                  ours_status=?28,
                  ours_meta_json=?29
                WHERE workspace=?1 AND conflict_id=?2 AND status != 'resolved'
                "#,
                params![
                    ctx.workspace,
                    existing.conflict_id.as_str(),
                    base.map(|n| n.last_seq),
                    base.map(|n| n.last_ts_ms),
                    base.map(|n| n.deleted as i64),
                    base.map(|n| n.node_type.as_str()),
                    base.and_then(|n| n.title.as_deref()),
                    base.and_then(|n| n.text.as_deref()),
                    base_tags.as_deref(),
                    base.and_then(|n| n.status.as_deref()),
                    base.and_then(|n| n.meta_json.as_deref()),
                    theirs_seq,
                    theirs.map(|n| n.last_ts_ms),
                    theirs.map(|n| n.deleted as i64),
                    theirs.map(|n| n.node_type.as_str()),
                    theirs.and_then(|n| n.title.as_deref()),
                    theirs.and_then(|n| n.text.as_deref()),
                    theirs_tags.as_deref(),
                    theirs.and_then(|n| n.status.as_deref()),
                    theirs.and_then(|n| n.meta_json.as_deref()),
                    ours_seq,
                    ours.map(|n| n.last_ts_ms),
                    ours.map(|n| n.deleted as i64),
                    ours.map(|n| n.node_type.as_str()),
                    ours.and_then(|n| n.title.as_deref()),
                    ours.and_then(|n| n.text.as_deref()),
                    ours_tags.as_deref(),
                    ours.and_then(|n| n.status.as_deref()),
                    ours.and_then(|n| n.meta_json.as_deref()),
                ],
            )?;
        }
        return Ok(GraphConflictCreateResult { inserted: false });
    }

    let conflict_id = graph_conflict_id(GraphConflictIdArgs {
        workspace: ctx.workspace,
        from_branch: ctx.from_branch,
        into_branch: ctx.into_branch,
        doc: ctx.doc,
        kind: "node",
        key,
        base_cutoff_seq: ctx.base_cutoff_seq,
        theirs_seq,
    });

    let base_tags = base.and_then(|n| encode_tags(&n.tags));
    let theirs_tags = theirs.and_then(|n| encode_tags(&n.tags));
    let ours_tags = ours.and_then(|n| encode_tags(&n.tags));

    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO graph_conflicts(
          workspace, conflict_id, kind, key, from_branch, into_branch, doc, base_cutoff_seq,
          base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
          base_from_id, base_rel, base_to_id, base_edge_meta_json,
          theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
          theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
          ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
          ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json,
          status, created_at_ms
        )
        VALUES (
          ?1, ?2, 'node', ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
          NULL, NULL, NULL, NULL,
          ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
          NULL, NULL, NULL, NULL,
          ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34,
          NULL, NULL, NULL, NULL,
          'open', ?35
        )
        "#,
        params![
            ctx.workspace,
            &conflict_id,
            key,
            ctx.from_branch,
            ctx.into_branch,
            ctx.doc,
            ctx.base_cutoff_seq,
            base.map(|n| n.last_seq),
            base.map(|n| n.last_ts_ms),
            base.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            base.map(|n| n.node_type.as_str()),
            base.and_then(|n| n.title.as_deref()),
            base.and_then(|n| n.text.as_deref()),
            base_tags,
            base.and_then(|n| n.status.as_deref()),
            base.and_then(|n| n.meta_json.as_deref()),
            theirs_seq,
            theirs.map(|n| n.last_ts_ms),
            theirs.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            theirs.map(|n| n.node_type.as_str()),
            theirs.and_then(|n| n.title.as_deref()),
            theirs.and_then(|n| n.text.as_deref()),
            theirs_tags,
            theirs.and_then(|n| n.status.as_deref()),
            theirs.and_then(|n| n.meta_json.as_deref()),
            ours_seq,
            ours.map(|n| n.last_ts_ms),
            ours.map(|n| if n.deleted { 1i64 } else { 0i64 }),
            ours.map(|n| n.node_type.as_str()),
            ours.and_then(|n| n.title.as_deref()),
            ours.and_then(|n| n.text.as_deref()),
            ours_tags,
            ours.and_then(|n| n.status.as_deref()),
            ours.and_then(|n| n.meta_json.as_deref()),
            ctx.now_ms
        ],
    )?;
    Ok(GraphConflictCreateResult {
        inserted: inserted > 0,
    })
}

pub(in crate::store) fn graph_conflict_create_edge_tx(
    tx: &Transaction<'_>,
    ctx: &GraphConflictCreateCtx<'_>,
    key: &GraphEdgeKey,
    base: Option<&GraphEdgeRow>,
    theirs: Option<&GraphEdgeRow>,
    ours: Option<&GraphEdgeRow>,
) -> Result<GraphConflictCreateResult, StoreError> {
    let key_str = format!("{}|{}|{}", key.from, key.rel, key.to);
    let theirs_seq = theirs.map(|e| e.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|e| e.last_seq).unwrap_or(0);

    if let Some(existing) = graph_conflict_status_row_by_signature_tx(
        tx,
        GraphConflictSignatureArgs {
            workspace: ctx.workspace,
            from_branch: ctx.from_branch,
            into_branch: ctx.into_branch,
            doc: ctx.doc,
            kind: "edge",
            key: key_str.as_str(),
            base_cutoff_seq: ctx.base_cutoff_seq,
            theirs_seq,
        },
    )? {
        if existing.status != "resolved" {
            tx.execute(
                r#"
                UPDATE graph_conflicts
                SET
                  base_deleted=?3,
                  base_from_id=?4,
                  base_rel=?5,
                  base_to_id=?6,
                  base_edge_meta_json=?7,

                  theirs_seq=?8,
                  theirs_ts_ms=?9,
                  theirs_deleted=?10,
                  theirs_from_id=?11,
                  theirs_rel=?12,
                  theirs_to_id=?13,
                  theirs_edge_meta_json=?14,

                  ours_seq=?15,
                  ours_ts_ms=?16,
                  ours_deleted=?17,
                  ours_from_id=?18,
                  ours_rel=?19,
                  ours_to_id=?20,
                  ours_edge_meta_json=?21
                WHERE workspace=?1 AND conflict_id=?2 AND status != 'resolved'
                "#,
                params![
                    ctx.workspace,
                    existing.conflict_id.as_str(),
                    base.map(|e| e.deleted as i64),
                    base.map(|e| e.from.as_str()),
                    base.map(|e| e.rel.as_str()),
                    base.map(|e| e.to.as_str()),
                    base.and_then(|e| e.meta_json.as_deref()),
                    theirs_seq,
                    theirs.map(|e| e.last_ts_ms),
                    theirs.map(|e| e.deleted as i64),
                    theirs.map(|e| e.from.as_str()),
                    theirs.map(|e| e.rel.as_str()),
                    theirs.map(|e| e.to.as_str()),
                    theirs.and_then(|e| e.meta_json.as_deref()),
                    ours_seq,
                    ours.map(|e| e.last_ts_ms),
                    ours.map(|e| e.deleted as i64),
                    ours.map(|e| e.from.as_str()),
                    ours.map(|e| e.rel.as_str()),
                    ours.map(|e| e.to.as_str()),
                    ours.and_then(|e| e.meta_json.as_deref()),
                ],
            )?;
        }
        return Ok(GraphConflictCreateResult { inserted: false });
    }
    let conflict_id = graph_conflict_id(GraphConflictIdArgs {
        workspace: ctx.workspace,
        from_branch: ctx.from_branch,
        into_branch: ctx.into_branch,
        doc: ctx.doc,
        kind: "edge",
        key: &key_str,
        base_cutoff_seq: ctx.base_cutoff_seq,
        theirs_seq,
    });

    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO graph_conflicts(
          workspace, conflict_id, kind, key, from_branch, into_branch, doc, base_cutoff_seq,
          base_seq, base_ts_ms, base_deleted, base_node_type, base_title, base_text, base_tags, base_status, base_meta_json,
          base_from_id, base_rel, base_to_id, base_edge_meta_json,
          theirs_seq, theirs_ts_ms, theirs_deleted, theirs_node_type, theirs_title, theirs_text, theirs_tags, theirs_status, theirs_meta_json,
          theirs_from_id, theirs_rel, theirs_to_id, theirs_edge_meta_json,
          ours_seq, ours_ts_ms, ours_deleted, ours_node_type, ours_title, ours_text, ours_tags, ours_status, ours_meta_json,
          ours_from_id, ours_rel, ours_to_id, ours_edge_meta_json,
          status, created_at_ms
        )
        VALUES (
          ?1, ?2, 'edge', ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, NULL, NULL, NULL, NULL, NULL, NULL,
          ?11, ?12, ?13, ?14,
          ?15, ?16, ?17, NULL, NULL, NULL, NULL, NULL, NULL,
          ?18, ?19, ?20, ?21,
          ?22, ?23, ?24, NULL, NULL, NULL, NULL, NULL, NULL,
          ?25, ?26, ?27, ?28,
          'open', ?29
        )
        "#,
        params![
            ctx.workspace,
            &conflict_id,
            &key_str,
            ctx.from_branch,
            ctx.into_branch,
            ctx.doc,
            ctx.base_cutoff_seq,
            base.map(|e| e.last_seq),
            base.map(|e| e.last_ts_ms),
            base.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            base.map(|e| e.from.as_str()),
            base.map(|e| e.rel.as_str()),
            base.map(|e| e.to.as_str()),
            base.and_then(|e| e.meta_json.as_deref()),
            theirs_seq,
            theirs.map(|e| e.last_ts_ms),
            theirs.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            theirs.map(|e| e.from.as_str()),
            theirs.map(|e| e.rel.as_str()),
            theirs.map(|e| e.to.as_str()),
            theirs.and_then(|e| e.meta_json.as_deref()),
            ours_seq,
            ours.map(|e| e.last_ts_ms),
            ours.map(|e| if e.deleted { 1i64 } else { 0i64 }),
            ours.map(|e| e.from.as_str()),
            ours.map(|e| e.rel.as_str()),
            ours.map(|e| e.to.as_str()),
            ours.and_then(|e| e.meta_json.as_deref()),
            ctx.now_ms
        ],
    )?;
    Ok(GraphConflictCreateResult {
        inserted: inserted > 0,
    })
}
