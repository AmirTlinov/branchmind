#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn think_card_compact_returns_refs_only() {
    let mut server = Server::start_initialized_with_args(
        "think_card_compact_returns_refs_only",
        &["--toolset", "daily", "--workspace", "ws_compact"],
    );

    let compact = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws_compact", "card": "Quick note", "verbosity": "compact" } } }
    }));
    let compact = extract_tool_text(&compact);
    assert_eq!(
        compact.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think_card compact should succeed"
    );
    let result = compact.get("result").unwrap();
    assert!(
        result.get("card_id").is_some(),
        "compact must include card_id"
    );
    assert!(
        result.get("trace_ref").is_some(),
        "compact must include trace_ref"
    );
    assert!(
        result.get("trace_doc").is_none(),
        "compact must omit trace_doc"
    );
    assert!(
        result.get("graph_doc").is_none(),
        "compact must omit graph_doc"
    );

    let full = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws_compact", "card": "Full note" } } }
    }));
    let full = extract_tool_text(&full);
    let full_result = full.get("result").unwrap();
    assert!(
        full_result.get("trace_doc").is_some(),
        "full must include trace_doc"
    );
    assert!(
        full_result.get("graph_doc").is_some(),
        "full must include graph_doc"
    );
}
