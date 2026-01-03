#![forbid(unsafe_code)]

use super::super::super::{
    GraphEdgeRow, GraphEdgeUpsert, GraphNodeRow, GraphNodeUpsert, GraphOp, StoreError,
};
use super::op_event::insert_graph_doc_entry_tx;
use super::types::{
    GraphEdgeUpsertTxArgs, GraphEdgeVersionInsertTxArgs, GraphNodeUpsertTxArgs,
    GraphNodeVersionInsertTxArgs,
};
use super::validate::{validate_graph_node_id, validate_graph_rel, validate_graph_type};
use super::versions::{insert_graph_edge_version_tx, insert_graph_node_version_tx};
use rusqlite::Transaction;

pub(in crate::store) fn graph_upsert_node_tx(
    tx: &Transaction<'_>,
    args: GraphNodeUpsertTxArgs<'_>,
) -> Result<bool, StoreError> {
    let GraphNodeUpsertTxArgs {
        workspace,
        branch,
        doc,
        now_ms,
        node_id,
        node_type,
        title,
        status,
        meta_json,
        source_event_id,
    } = args;

    validate_graph_node_id(node_id)?;
    validate_graph_type(node_type)?;

    let op = GraphOp::NodeUpsert(GraphNodeUpsert {
        id: node_id.to_string(),
        node_type: node_type.to_string(),
        title: title.map(|v| v.to_string()),
        text: None,
        tags: Vec::new(),
        status: status.map(|v| v.to_string()),
        meta_json: meta_json.map(|v| v.to_string()),
    });
    let (_payload, seq_opt) = insert_graph_doc_entry_tx(
        tx,
        workspace,
        branch,
        doc,
        now_ms,
        &op,
        Some(source_event_id),
    )?;
    let Some(seq) = seq_opt else {
        return Ok(false);
    };

    insert_graph_node_version_tx(
        tx,
        GraphNodeVersionInsertTxArgs {
            workspace,
            branch,
            doc,
            seq,
            ts_ms: now_ms,
            node_id,
            node_type: Some(node_type),
            title,
            text: None,
            tags: &[],
            status,
            meta_json,
            deleted: false,
        },
    )?;
    Ok(true)
}

pub(in crate::store) fn graph_upsert_edge_tx(
    tx: &Transaction<'_>,
    args: GraphEdgeUpsertTxArgs<'_>,
) -> Result<bool, StoreError> {
    let GraphEdgeUpsertTxArgs {
        workspace,
        branch,
        doc,
        now_ms,
        from,
        rel,
        to,
        meta_json,
        source_event_id,
    } = args;

    validate_graph_node_id(from)?;
    validate_graph_node_id(to)?;
    validate_graph_rel(rel)?;

    let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
        from: from.to_string(),
        rel: rel.to_string(),
        to: to.to_string(),
        meta_json: meta_json.map(|v| v.to_string()),
    });
    let (_payload, seq_opt) = insert_graph_doc_entry_tx(
        tx,
        workspace,
        branch,
        doc,
        now_ms,
        &op,
        Some(source_event_id),
    )?;
    let Some(seq) = seq_opt else {
        return Ok(false);
    };

    insert_graph_edge_version_tx(
        tx,
        GraphEdgeVersionInsertTxArgs {
            workspace,
            branch,
            doc,
            seq,
            ts_ms: now_ms,
            from_id: from,
            rel,
            to_id: to,
            meta_json,
            deleted: false,
        },
    )?;
    Ok(true)
}

pub(in crate::store) fn graph_node_semantic_eq(
    left: Option<&GraphNodeRow>,
    right: Option<&GraphNodeRow>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(a), Some(b)) => {
            a.id == b.id
                && a.deleted == b.deleted
                && a.node_type == b.node_type
                && a.title == b.title
                && a.text == b.text
                && a.tags == b.tags
                && a.status == b.status
                && a.meta_json.as_deref().map(str::trim) == b.meta_json.as_deref().map(str::trim)
        }
    }
}

pub(in crate::store) fn graph_edge_semantic_eq(
    left: Option<&GraphEdgeRow>,
    right: Option<&GraphEdgeRow>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
        (Some(a), Some(b)) => {
            a.from == b.from
                && a.rel == b.rel
                && a.to == b.to
                && a.deleted == b.deleted
                && a.meta_json.as_deref().map(str::trim) == b.meta_json.as_deref().map(str::trim)
        }
    }
}

pub(in crate::store) fn count_node_field_changes(
    base: Option<&GraphNodeRow>,
    theirs: &GraphNodeRow,
) -> usize {
    let mut changed = 0usize;
    if base.map(|n| n.node_type.as_str()) != Some(theirs.node_type.as_str()) {
        changed += 1;
    }
    if base.and_then(|n| n.title.as_deref()) != theirs.title.as_deref() {
        changed += 1;
    }
    if base.and_then(|n| n.text.as_deref()) != theirs.text.as_deref() {
        changed += 1;
    }
    let base_tags = base.map(|n| n.tags.as_slice());
    if base_tags != Some(theirs.tags.as_slice()) {
        changed += 1;
    }
    if base.and_then(|n| n.status.as_deref()) != theirs.status.as_deref() {
        changed += 1;
    }
    if base.and_then(|n| n.meta_json.as_deref()).map(str::trim)
        != theirs.meta_json.as_deref().map(str::trim)
    {
        changed += 1;
    }
    if base.map(|n| n.deleted) != Some(theirs.deleted) {
        changed += 1;
    }
    changed
}

pub(in crate::store) fn count_edge_field_changes(
    base: Option<&GraphEdgeRow>,
    theirs: &GraphEdgeRow,
) -> usize {
    let mut changed = 0usize;
    if base.and_then(|e| e.meta_json.as_deref()).map(str::trim)
        != theirs.meta_json.as_deref().map(str::trim)
    {
        changed += 1;
    }
    if base.map(|e| e.deleted) != Some(theirs.deleted) {
        changed += 1;
    }
    changed
}
