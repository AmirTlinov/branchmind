#![forbid(unsafe_code)]

use super::json::parse_json_or_string;
use bm_storage::{GraphEdge, GraphNode};
use serde_json::{Value, json};

pub(crate) fn graph_nodes_to_cards(nodes: Vec<GraphNode>) -> Vec<Value> {
    let mut nodes = nodes;
    nodes.sort_by(|a, b| {
        b.last_ts_ms
            .cmp(&a.last_ts_ms)
            .then_with(|| b.last_seq.cmp(&a.last_seq))
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes
        .into_iter()
        .map(|n| {
            json!({
                "id": n.id,
                "type": n.node_type,
                "title": n.title,
                "text": n.text,
                "status": n.status,
                "tags": n.tags,
                "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                "deleted": n.deleted,
                "last_seq": n.last_seq,
                "last_ts_ms": n.last_ts_ms
            })
        })
        .collect()
}

pub(crate) fn graph_edges_to_json(edges: Vec<GraphEdge>) -> Vec<Value> {
    let mut edges = edges;
    edges.sort_by(|a, b| {
        b.last_ts_ms
            .cmp(&a.last_ts_ms)
            .then_with(|| b.last_seq.cmp(&a.last_seq))
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.rel.cmp(&b.rel))
            .then_with(|| a.to.cmp(&b.to))
    });

    edges.into_iter()
        .map(|e| {
            json!({
                "from": e.from,
                "rel": e.rel,
                "to": e.to,
                "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                "deleted": e.deleted,
                "last_seq": e.last_seq,
                "last_ts_ms": e.last_ts_ms
            })
        })
        .collect()
}

pub(crate) fn graph_nodes_to_signal_cards(nodes: Vec<GraphNode>) -> Vec<Value> {
    let mut nodes = nodes;
    nodes.sort_by(|a, b| {
        b.last_ts_ms
            .cmp(&a.last_ts_ms)
            .then_with(|| b.last_seq.cmp(&a.last_seq))
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes
        .into_iter()
        .map(|n| {
            json!({
                "id": n.id,
                "type": n.node_type,
                "title": n.title,
                "text": n.text,
                "status": n.status,
                "tags": n.tags,
                "last_seq": n.last_seq,
                "last_ts_ms": n.last_ts_ms
            })
        })
        .collect()
}

pub(crate) fn merge_meta_with_message(
    existing_meta: Option<&str>,
    message: Option<String>,
    extra_meta: Option<String>,
) -> Option<String> {
    let mut out = serde_json::Map::new();
    if let Some(raw) = existing_meta
        && let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw)
    {
        for (k, v) in obj {
            out.insert(k, v);
        }
    }
    if let Some(raw) = extra_meta
        && let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&raw)
    {
        for (k, v) in obj {
            out.insert(k, v);
        }
    }
    if let Some(message) = message
        && !message.trim().is_empty()
    {
        out.insert("message".to_string(), Value::String(message));
    }
    if out.is_empty() {
        None
    } else {
        Some(Value::Object(out).to_string())
    }
}
