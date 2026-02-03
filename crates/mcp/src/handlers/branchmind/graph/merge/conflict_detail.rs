#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(super) fn conflict_detail_to_json(detail: &bm_storage::GraphConflictDetail) -> Value {
        let base = if detail.kind == "node" {
            detail
                .base_node
                .as_ref()
                .map(|n| {
                    json!({
                        "id": n.id.clone(),
                        "type": n.node_type.clone(),
                        "title": n.title.clone(),
                        "text": n.text.clone(),
                        "status": n.status.clone(),
                        "tags": n.tags.clone(),
                        "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": n.deleted,
                        "last_seq": n.last_seq,
                        "last_ts_ms": n.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        } else {
            detail
                .base_edge
                .as_ref()
                .map(|e| {
                    json!({
                        "from": e.from.clone(),
                        "rel": e.rel.clone(),
                        "to": e.to.clone(),
                        "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": e.deleted,
                        "last_seq": e.last_seq,
                        "last_ts_ms": e.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        };
        let theirs = if detail.kind == "node" {
            detail
                .theirs_node
                .as_ref()
                .map(|n| {
                    json!({
                        "id": n.id.clone(),
                        "type": n.node_type.clone(),
                        "title": n.title.clone(),
                        "text": n.text.clone(),
                        "status": n.status.clone(),
                        "tags": n.tags.clone(),
                        "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": n.deleted,
                        "last_seq": n.last_seq,
                        "last_ts_ms": n.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        } else {
            detail
                .theirs_edge
                .as_ref()
                .map(|e| {
                    json!({
                        "from": e.from.clone(),
                        "rel": e.rel.clone(),
                        "to": e.to.clone(),
                        "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": e.deleted,
                        "last_seq": e.last_seq,
                        "last_ts_ms": e.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        };
        let ours = if detail.kind == "node" {
            detail
                .ours_node
                .as_ref()
                .map(|n| {
                    json!({
                        "id": n.id.clone(),
                        "type": n.node_type.clone(),
                        "title": n.title.clone(),
                        "text": n.text.clone(),
                        "status": n.status.clone(),
                        "tags": n.tags.clone(),
                        "meta": n.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": n.deleted,
                        "last_seq": n.last_seq,
                        "last_ts_ms": n.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        } else {
            detail
                .ours_edge
                .as_ref()
                .map(|e| {
                    json!({
                        "from": e.from.clone(),
                        "rel": e.rel.clone(),
                        "to": e.to.clone(),
                        "meta": e.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                        "deleted": e.deleted,
                        "last_seq": e.last_seq,
                        "last_ts_ms": e.last_ts_ms
                    })
                })
                .unwrap_or(Value::Null)
        };

        json!({
            "conflict_id": detail.conflict_id.clone(),
            "kind": detail.kind.clone(),
            "key": detail.key.clone(),
            "from": detail.from_branch.clone(),
            "into": detail.into_branch.clone(),
            "doc": detail.doc.clone(),
            "status": detail.status.clone(),
            "created_at_ms": detail.created_at_ms,
            "resolved_at_ms": detail.resolved_at_ms,
            "base": base,
            "theirs": theirs,
            "ours": ours
        })
    }
}
