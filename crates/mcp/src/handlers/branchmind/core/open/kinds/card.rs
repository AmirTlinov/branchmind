#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_card(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    card_id: &str,
    include_content: bool,
) -> Result<Value, Value> {
    let opened = match server.store.graph_card_open_by_id(workspace, card_id) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(StoreError::UnknownBranch) => {
            return Err(ai_error(
                "UNKNOWN_ID",
                "Unknown branch for the requested card",
            ));
        }
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(opened) = opened else {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown card id",
            Some("Copy a CARD-* id from snapshot delta or a prior think_* response."),
            vec![],
        ));
    };

    let card = json!({
        "id": opened.node.id,
        "type": opened.node.node_type,
        "title": opened.node.title,
        "text": opened.node.text,
        "status": opened.node.status.unwrap_or_else(|| "open".to_string()),
        "tags": opened.node.tags,
        "meta": opened.node.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
    });

    let mut out = json!({
        "workspace": workspace.as_str(),
        "kind": "card",
        "id": card_id,
        "head": {
            "seq": opened.head.seq,
            "ts": ts_ms_to_rfc3339(opened.head.ts_ms),
            "ts_ms": opened.head.ts_ms,
            "branch": opened.head.branch,
            "doc": opened.head.doc
        },
        "card": card,
        "edges": {
            "supports": opened.supports,
            "blocks": opened.blocks
        },
        "summary": super::super::util::summary_one_line(
            opened.node.text.as_deref(),
            opened.node.title.as_deref(),
            120
        ),
        "truncated": false
    });

    if include_content && let Some(obj) = out.as_object_mut() {
        obj.insert(
            "content".to_string(),
            json!({
                "title": card.get("title").cloned().unwrap_or(Value::Null),
                "text": card.get("text").cloned().unwrap_or(Value::Null)
            }),
        );
    }

    Ok(out)
}
