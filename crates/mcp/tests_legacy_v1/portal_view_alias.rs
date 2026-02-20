#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn portal_view_is_supported_and_conflicts_with_view() {
    let mut server = Server::start_initialized("portal_view_is_supported_and_conflicts_with_view");

    let ok = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": {
            "workspace": "ws_portal_view_alias",
            "op": "call",
            "cmd": "tasks.plan.create",
            "args": { "kind": "plan", "title": "Portal view plan" },
            "budget_profile": "portal",
            "portal_view": "compact"
        } }
    }));
    let ok = extract_tool_text(&ok);
    assert_eq!(ok.get("success").and_then(|v| v.as_bool()), Some(true));

    let conflict = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": {
            "workspace": "ws_portal_view_alias",
            "op": "call",
            "cmd": "tasks.plan.create",
            "args": { "kind": "plan", "title": "Portal view plan 2" },
            "budget_profile": "portal",
            "portal_view": "compact",
            "view": "smart"
        } }
    }));
    let conflict = extract_tool_text(&conflict);
    assert!(
        !conflict
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "expected INVALID_INPUT error: {conflict}"
    );
    assert_eq!(
        conflict
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}
