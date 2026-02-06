#![forbid(unsafe_code)]

use crate::ops::{Envelope, OpError, OpResponse};
use serde_json::Value;

pub(super) fn handle_note_promote(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(ws) = env.workspace.as_deref() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "workspace is required".to_string(),
                recovery: Some(
                    "Call workspace op=use first (or configure default workspace).".to_string(),
                ),
            },
        );
    };
    let workspace = match crate::WorkspaceId::try_new(ws.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some("Use workspace like my-workspace".to_string()),
                },
            );
        }
    };

    if !server.note_promote_enabled {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "FEATURE_DISABLED".to_string(),
                message: "note promotion is disabled".to_string(),
                recovery: Some(
                    "Enable via --note-promote (or env BRANCHMIND_NOTE_PROMOTE=1).".to_string(),
                ),
            },
        );
    }

    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: None,
            },
        );
    };

    let note_ref = match args_obj.get("note_ref").and_then(|v| v.as_str()) {
        Some(v) => v.trim().to_string(),
        None => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "note_ref is required".to_string(),
                    recovery: Some("Provide note_ref like notes@123".to_string()),
                },
            );
        }
    };
    let Some((doc, seq)) = parse_doc_entry_ref(&note_ref) else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "note_ref must be <doc>@<seq> (e.g. notes@123)".to_string(),
                recovery: None,
            },
        );
    };

    let entry = match server.store.doc_entry_get_by_seq(&workspace, seq) {
        Ok(v) => v,
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: None,
                },
            );
        }
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("store error: {err}"),
                    recovery: None,
                },
            );
        }
    };
    let Some(entry) = entry else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "UNKNOWN_ID".to_string(),
                message: "note_ref not found".to_string(),
                recovery: Some("Use notes@seq from a prior notes_commit response.".to_string()),
            },
        );
    };
    if entry.doc != doc {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "doc prefix mismatch for note_ref".to_string(),
                recovery: Some(format!("Expected {}@{}", entry.doc, entry.seq)),
            },
        );
    }

    let note_text = entry.content.clone().unwrap_or_default();
    if note_text.trim().is_empty() {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "note content is empty".to_string(),
                recovery: None,
            },
        );
    }

    let title_override = args_obj
        .get("title")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let title = title_override.or(entry.title.clone());

    let mut key_mode = args_obj
        .get("key_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("auto")
        .trim()
        .to_ascii_lowercase();
    if !matches!(key_mode.as_str(), "explicit" | "auto") {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "key_mode must be explicit|auto".to_string(),
                recovery: Some("Use key_mode=\"explicit\" or key_mode=\"auto\".".to_string()),
            },
        );
    }
    let lint_mode = args_obj
        .get("lint_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("manual")
        .trim()
        .to_ascii_lowercase();
    if !matches!(lint_mode.as_str(), "manual" | "auto") {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "lint_mode must be manual|auto".to_string(),
                recovery: Some("Use lint_mode=\"manual\" or lint_mode=\"auto\".".to_string()),
            },
        );
    }

    let anchor = args_obj
        .get("anchor")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    if key_mode == "auto" && anchor.is_none() {
        key_mode = "explicit".to_string();
    }
    let key = args_obj
        .get("key")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let mut card_obj = serde_json::Map::new();
    if let Some(title) = title.clone() {
        card_obj.insert("title".to_string(), Value::String(title));
    }
    card_obj.insert("text".to_string(), Value::String(note_text));
    card_obj.insert(
        "tags".to_string(),
        Value::Array(vec![Value::String(crate::VIS_TAG_DRAFT.to_string())]),
    );

    let mut forwarded = serde_json::Map::new();
    if let Some(anchor) = anchor {
        forwarded.insert("anchor".to_string(), Value::String(anchor));
    }
    if let Some(key) = key {
        forwarded.insert("key".to_string(), Value::String(key));
    }
    forwarded.insert("key_mode".to_string(), Value::String(key_mode));
    forwarded.insert("lint_mode".to_string(), Value::String(lint_mode));
    forwarded.insert("card".to_string(), Value::Object(card_obj));

    let sub_env = Envelope {
        workspace: env.workspace.clone(),
        cmd: env.cmd.clone(),
        args: Value::Object(forwarded),
    };
    let mut resp = super::knowledge::handle_knowledge_upsert(server, &sub_env);
    if resp.error.is_none()
        && let Some(obj) = resp.result.as_object_mut()
    {
        obj.insert("note_ref".to_string(), Value::String(note_ref));
    }
    resp
}

fn parse_doc_entry_ref(raw: &str) -> Option<(String, i64)> {
    let raw = raw.trim();
    let (doc, seq_str) = raw.rsplit_once('@')?;
    let doc = doc.trim();
    let seq_str = seq_str.trim();
    if doc.is_empty() || seq_str.is_empty() {
        return None;
    }
    let seq = seq_str.parse::<i64>().ok()?;
    if seq < 0 {
        return None;
    }
    Some((doc.to_string(), seq))
}
