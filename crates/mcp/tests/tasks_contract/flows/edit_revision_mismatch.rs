#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_edit_revision_mismatch() {
    let mut server = Server::start_initialized("tasks_edit_revision_mismatch");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let mismatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_edit", "arguments": { "workspace": "ws1", "task": plan_id, "expected_revision": 999, "title": "Nope" } }
    }));
    assert_eq!(
        mismatch
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let mismatch_text = extract_tool_text(&mismatch);
    assert_eq!(
        mismatch_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("REVISION_MISMATCH")
    );

    let actions = mismatch_text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions");
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("tasks")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("tasks.context")
        }),
        "REVISION_MISMATCH must include an actionable follow-up (tasks.context)"
    );

    let delta = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1" } }
    }));
    let delta_text = extract_tool_text(&delta);
    let events = delta_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 1);
}
