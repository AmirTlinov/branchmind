#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_upsert_invalid_key_includes_suggestion_and_action() {
    let mut server = Server::start_initialized_with_args(
        "knowledge_upsert_invalid_key_includes_suggestion_and_action",
        &["--toolset", "daily", "--workspace", "ws_kb_invalid_key"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.knowledge.upsert", "args": {
            "anchor": "core",
            "key": "Bad Key!",
            "card": { "title": "Determinism invariants", "text": "Claim: ...\nScope: core\nApply: ...\nProof: CMD: make check\nExpiry: 2027-01-01" }
        } } }
    }));

    let out = extract_tool_text(&resp);
    assert!(
        !out.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
        "expected INVALID_INPUT error: {out}"
    );

    let recovery = out
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        recovery.contains("Suggested key:"),
        "recovery should include a suggested key: {recovery}"
    );
    assert!(
        recovery.contains("bad-key"),
        "suggested key should be deterministic slug: {recovery}"
    );

    let actions = out
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("args")
                .and_then(|v| v.get("cmd"))
                .and_then(|v| v.as_str())
                == Some("think.knowledge.key.suggest")
        }),
        "expected an action pointing to think.knowledge.key.suggest: {actions:?}"
    );
}
