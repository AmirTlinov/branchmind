#![forbid(unsafe_code)]

use super::super::super::{
    GraphEdgeRow, GraphEdgeUpsert, GraphNodeRow, GraphNodeUpsert, GraphOp, StoreError,
};
use super::super::branch_sources_tx;
use super::op_event::insert_graph_doc_entry_tx;
use super::query::{graph_edge_keys_for_node_tx, graph_node_get_tx};
use super::types::{
    GraphEdgeUpsertTxArgs, GraphEdgeVersionInsertTxArgs, GraphNodeDeleteTxArgs,
    GraphNodeUpsertTxArgs, GraphNodeVersionInsertTxArgs,
};
use super::validate::{validate_graph_node_id, validate_graph_rel, validate_graph_type};
use super::versions::{insert_graph_edge_version_tx, insert_graph_node_version_tx};
use rusqlite::Transaction;
use serde_json::Value;

const RESERVED_META_KEYS: [&str; 3] = ["_merge", "_meta", "_meta_raw"];

fn meta_json_semantic_eq(left: Option<&str>, right: Option<&str>) -> bool {
    normalize_meta_json(left) == normalize_meta_json(right)
}

fn normalize_meta_json(value: Option<&str>) -> Option<Value> {
    let trimmed = value.map(str::trim).filter(|s| !s.is_empty())?;

    let mut v: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Some(Value::String(trimmed.to_string())),
    };
    strip_reserved_meta_keys(&mut v);

    match v {
        Value::Null => None,
        Value::Object(ref map) if map.is_empty() => None,
        _ => Some(v),
    }
}

fn strip_reserved_meta_keys(value: &mut Value) {
    let Value::Object(map) = value else { return };
    for k in RESERVED_META_KEYS {
        map.remove(k);
    }
}

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

pub(in crate::store) fn graph_delete_node_tx(
    tx: &Transaction<'_>,
    args: GraphNodeDeleteTxArgs<'_>,
) -> Result<bool, StoreError> {
    let GraphNodeDeleteTxArgs {
        workspace,
        branch,
        doc,
        now_ms,
        node_id,
        source_event_id,
    } = args;

    validate_graph_node_id(node_id)?;

    let op = GraphOp::NodeDelete {
        id: node_id.to_string(),
    };
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

    let sources = branch_sources_tx(tx, workspace, branch)?;
    let existing = graph_node_get_tx(tx, workspace, &sources, doc, node_id)?;

    let inferred_type = if node_id.starts_with("task:") {
        Some("task")
    } else if node_id.starts_with("step:") {
        Some("step")
    } else {
        None
    };

    let mut tags = Vec::new();
    let (node_type, title, text, status, meta_json) = if let Some(existing) = existing {
        tags = existing.tags;
        let node_type = if existing.node_type.trim().is_empty() {
            None
        } else {
            Some(existing.node_type)
        };
        (
            node_type,
            existing.title,
            existing.text,
            existing.status,
            existing.meta_json,
        )
    } else {
        (
            inferred_type.map(|value| value.to_string()),
            None,
            None,
            None,
            None,
        )
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
            node_type: node_type.as_deref(),
            title: title.as_deref(),
            text: text.as_deref(),
            tags: &tags,
            status: status.as_deref(),
            meta_json: meta_json.as_deref(),
            deleted: true,
        },
    )?;

    let edge_keys = graph_edge_keys_for_node_tx(tx, workspace, &sources, doc, node_id)?;
    for key in edge_keys {
        insert_graph_edge_version_tx(
            tx,
            GraphEdgeVersionInsertTxArgs {
                workspace,
                branch,
                doc,
                seq,
                ts_ms: now_ms,
                from_id: key.from.as_str(),
                rel: key.rel.as_str(),
                to_id: key.to.as_str(),
                meta_json: None,
                deleted: true,
            },
        )?;
    }

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
                && meta_json_semantic_eq(a.meta_json.as_deref(), b.meta_json.as_deref())
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
                && meta_json_semantic_eq(a.meta_json.as_deref(), b.meta_json.as_deref())
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
    if !meta_json_semantic_eq(
        base.and_then(|n| n.meta_json.as_deref()),
        theirs.meta_json.as_deref(),
    ) {
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
    if !meta_json_semantic_eq(
        base.and_then(|e| e.meta_json.as_deref()),
        theirs.meta_json.as_deref(),
    ) {
        changed += 1;
    }
    if base.map(|e| e.deleted) != Some(theirs.deleted) {
        changed += 1;
    }
    changed
}
