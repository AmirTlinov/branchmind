#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn invalid_input_returns_schema_actions() {
    let mut server = Server::start_initialized("invalid_input_returns_schema_actions");

    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": {} }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
    let empty_actions = Vec::new();
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_actions);
    assert!(
        actions.len() >= 2,
        "invalid input must return schema/get + example call actions"
    );
}

#[test]
fn invalid_input_returns_schema_actions_for_alias_ops_too() {
    let mut server =
        Server::start_initialized("invalid_input_returns_schema_actions_for_alias_ops_too");

    // Regression: parse-time INVALID_INPUT for alias ops (op != call) must still include
    // schema-on-demand recovery actions.
    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": { "workspace": "ws_alias_invalid", "op": "reasoning.seed", "args": "not-an-object" }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    let empty_actions = Vec::new();
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_actions);
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("system")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("schema.get")
        }),
        "expected system schema.get recovery action"
    );
}

#[test]
fn status_matches_execute_next_actions() {
    let mut server = Server::start_initialized("status_matches_execute_next_actions");

    let status = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": "ws_next" } }
    }));
    let status_text = extract_tool_text(&status);
    let status_actions = status_text.get("actions").cloned().unwrap_or(json!([]));

    let next = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": { "workspace": "ws_next", "op": "execute.next", "args": {} }
        }
    }));
    let next_text = extract_tool_text(&next);
    let next_actions = next_text.get("actions").cloned().unwrap_or(json!([]));

    assert_eq!(
        status_actions, next_actions,
        "status and execute.next actions must match"
    );
}
