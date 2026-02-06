#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn tools_call(
    server: &mut Server,
    id: i64,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    }))
}

fn any_string_contains(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains(needle),
        serde_json::Value::Array(arr) => arr.iter().any(|v| any_string_contains(v, needle)),
        serde_json::Value::Object(obj) => obj.values().any(|v| any_string_contains(v, needle)),
        _ => false,
    }
}

#[test]
fn next_engine_actions_are_runnable_and_do_not_use_placeholders_in_empty_workspace() {
    let mut server = Server::start_initialized_with_args(
        "next_engine_actions_are_runnable_and_do_not_use_placeholders_in_empty_workspace",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_next_engine_empty",
            "--response-verbosity",
            "full",
        ],
    );

    // No focus + empty workspace → NextEngine must still propose runnable actions.
    let next = tools_call(
        &mut server,
        1,
        "tasks",
        json!({ "op": "execute.next", "args": {} }),
    );
    let payload = extract_tool_text(&next);
    let actions = payload
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions array");
    assert!(
        !actions.is_empty(),
        "expected at least one NextEngine action"
    );

    // Flagship UX invariant: no "<placeholder>" strings in actions.
    assert!(
        !any_string_contains(&payload, "<"),
        "NextEngine payload must not contain placeholder markers, got:\n{payload}"
    );

    // First action should be a runnable golden start (macro.start).
    let first = &actions[0];
    assert_eq!(first.get("tool").and_then(|v| v.as_str()), Some("tasks"));
    let first_args = first.get("args").cloned().unwrap_or(json!({}));
    assert_eq!(
        first_args.get("cmd").and_then(|v| v.as_str()),
        Some("tasks.macro.start"),
        "expected a runnable start action, got: {first_args}"
    );

    // Execute the action as-is (copy/paste validity at the action layer).
    let started = tools_call(&mut server, 2, "tasks", first_args);
    let started_text = extract_tool_text_str(&started);
    assert!(
        !started_text.contains("ERROR:"),
        "start action must succeed, got:\n{started_text}"
    );
}
