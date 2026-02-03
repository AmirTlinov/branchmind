#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_doc_entry_compact_includes_budget() {
    let mut server = Server::start_initialized_with_args(
        "open_doc_entry_compact_includes_budget",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_open_doc_entry_budget",
        ],
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": {
            "workspace": "ws_open_doc_entry_budget",
            "content": "hello"
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

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": note_ref, "verbosity": "compact" } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(doc_entry) must succeed: {opened}"
    );
    let result = opened.get("result").expect("open result");
    assert!(
        result.get("budget").is_some(),
        "open compact must include budget for doc_entry"
    );
}
