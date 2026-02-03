#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_trace_doc_entry_ref_is_supported_for_task_prefixed_docs() {
    let mut server = Server::start_initialized_with_args(
        "open_trace_doc_entry_ref_is_supported_for_task_prefixed_docs",
        &[
            "--toolset",
            "full",
            "--workspace",
            "ws_open_trace_doc_entry",
        ],
    );

    let _started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
                "workspace": "ws_open_trace_doc_entry",
                "task_title": "Open trace doc entry ref",
                "steps": [
                    { "title": "S1", "success_criteria": ["ok"], "tests": ["noop"] }
                ]
            } } }
    }));

    let committed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_open_trace_doc_entry",
                "step": "focus",
                "card": { "type": "update", "title": "Trace entry", "text": "hello" }
            } } }
    }));
    let committed_out = extract_tool_text(&committed);
    assert!(
        committed_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "think_card must succeed: {committed_out}"
    );

    let trace_doc = committed_out
        .get("result")
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("think_card result.trace_doc")
        .to_string();
    assert!(
        trace_doc.starts_with("TASK-") && trace_doc.ends_with("-trace"),
        "expected task-prefixed trace doc, got: {trace_doc}"
    );

    let trace_ref = committed_out
        .get("result")
        .and_then(|v| v.get("trace_ref"))
        .and_then(|v| v.as_str())
        .expect("think_card result.trace_ref")
        .to_string();
    assert!(
        trace_ref.starts_with(&format!("{trace_doc}@")),
        "expected trace_ref to match trace_doc prefix, got: {trace_ref}"
    );

    let seq = committed_out
        .get("result")
        .and_then(|v| v.get("trace_seq"))
        .and_then(|v| v.as_i64())
        .expect("think_card result.trace_seq");

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": trace_ref } }
    }));
    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(trace_doc@seq) must succeed: {opened_out}"
    );

    let result = opened_out.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("doc_entry"),
        "expected kind=doc_entry"
    );
    assert_eq!(
        result.get("ref").and_then(|v| v.as_str()),
        Some(trace_ref.as_str()),
        "open must preserve ref"
    );
    assert_eq!(
        result
            .get("entry")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some(trace_doc.as_str()),
        "open doc_entry must match trace doc prefix"
    );
    assert_eq!(
        result
            .get("entry")
            .and_then(|v| v.get("seq"))
            .and_then(|v| v.as_i64())
            .unwrap_or(-1),
        seq,
        "open doc_entry must return the exact seq"
    );
}
