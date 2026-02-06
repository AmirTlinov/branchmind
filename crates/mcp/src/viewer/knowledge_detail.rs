#![forbid(unsafe_code)]

use super::ViewerConfig;
use super::detail::resolve_workspace;
use super::snapshot::SnapshotError;
use super::snapshot::store_err;
use crate::{now_ms_i64, now_rfc3339};
use bm_storage::SqliteStore;
use serde_json::{Value, json};

const DEFAULT_MAX_CHARS: usize = 20_000;
const MAX_MAX_CHARS: usize = 200_000;

pub(crate) fn build_knowledge_card_detail(
    store: &mut SqliteStore,
    config: &ViewerConfig,
    workspace_override: Option<&str>,
    card_id: &str,
    max_chars: Option<usize>,
) -> Result<Value, SnapshotError> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err(SnapshotError {
            code: "INVALID_CARD_ID",
            message: "card_id: must not be empty.".to_string(),
            recovery: Some("Provide a card id like CARD-KN-...".to_string()),
            status: 400,
        });
    }

    let resolved = resolve_workspace(store, config, workspace_override)?;
    let workspace = resolved.workspace.clone();

    let max_chars = max_chars
        .unwrap_or(DEFAULT_MAX_CHARS)
        .clamp(0, MAX_MAX_CHARS);

    let Some(opened) = store
        .graph_card_open_by_id(&workspace, card_id)
        .map_err(store_err("STORE_ERROR"))?
    else {
        return Err(SnapshotError {
            code: "UNKNOWN_CARD",
            message: "Card not found.".to_string(),
            recovery: Some(
                "Pick an existing card_id (e.g., from /api/search lens=knowledge).".to_string(),
            ),
            status: 404,
        });
    };

    let (text, truncated) = match opened.node.text.as_deref() {
        Some(text) => truncate_chars(text, max_chars),
        None => (None, false),
    };

    Ok(json!({
        "workspace": workspace.as_str(),
        "project_guard": {
            "expected": config.project_guard.as_deref(),
            "stored": resolved.stored_guard.as_deref(),
            "status": resolved.guard_status
        },
        "generated_at": now_rfc3339(),
        "generated_at_ms": now_ms_i64(),
        "card": {
            "id": opened.node.id,
            "type": opened.node.node_type,
            "title": opened.node.title,
            "text": text,
            "tags": opened.node.tags,
            "status": opened.node.status,
            "meta_json": opened.node.meta_json,
            "deleted": opened.node.deleted,
            "last_seq": opened.node.last_seq,
            "last_ts_ms": opened.node.last_ts_ms
        },
        "supports": opened.supports,
        "blocks": opened.blocks,
        "truncated": truncated,
        "limits": { "max_chars": max_chars }
    }))
}

fn truncate_chars(value: &str, max_chars: usize) -> (Option<String>, bool) {
    if max_chars == 0 {
        return (Some(String::new()), !value.is_empty());
    }

    let mut iter = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match iter.next() {
            Some(ch) => out.push(ch),
            None => return (Some(value.to_string()), false),
        }
    }

    // If there are remaining chars, mark truncation.
    if iter.next().is_some() {
        out.push('…');
        return (Some(out), true);
    }

    (Some(value.to_string()), false)
}
