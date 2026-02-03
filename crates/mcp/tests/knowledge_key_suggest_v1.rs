#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_key_suggest_returns_slug() {
    let mut server = Server::start_initialized_with_args(
        "knowledge_key_suggest_returns_slug",
        &["--workspace", "ws_key_suggest"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.knowledge.key.suggest", "args": {
            "workspace": "ws_key_suggest",
            "anchor": "core",
            "title": "Determinism invariants"
        } } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "think.knowledge.key.suggest must succeed: {out}"
    );
    let suggested = out
        .get("result")
        .and_then(|v| v.get("suggested_key"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(!suggested.is_empty(), "suggested_key must be returned");
    let key_tag = out
        .get("result")
        .and_then(|v| v.get("key_tag"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        key_tag.starts_with("k:"),
        "key_tag must be prefixed with k:"
    );
}

#[test]
fn note_promote_creates_knowledge_card() {
    let mut server = Server::start_initialized_with_args(
        "note_promote_creates_knowledge_card",
        &["--workspace", "ws_note_promote"],
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": {
            "workspace": "ws_note_promote",
            "content": "Claim: deterministic behavior"
        } } }
    }));
    let note_out = extract_tool_text(&note);
    assert!(
        note_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "notes_commit must succeed: {note_out}"
    );
    let seq = note_out
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("note seq");
    let note_ref = format!("notes@{seq}");

    let promoted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.note.promote", "args": {
            "workspace": "ws_note_promote",
            "note_ref": note_ref,
            "anchor": "core"
        } } }
    }));
    let promoted_out = extract_tool_text(&promoted);
    assert!(
        promoted_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "think.note.promote must succeed: {promoted_out}"
    );
    assert!(
        promoted_out
            .get("result")
            .and_then(|v| v.get("card_id"))
            .is_some(),
        "note promotion must return card_id"
    );
}
