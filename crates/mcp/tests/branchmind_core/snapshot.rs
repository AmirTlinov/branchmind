#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn snapshot_and_macro_branch_note_smoke() {
    let mut server = Server::start_initialized("snapshot_macro_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_snapshot", "kind": "plan", "title": "Plan A" } }
    }));
    let plan_id = extract_tool_text(&created_plan)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_snapshot", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let task_id = extract_tool_text(&created_task)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_snapshot", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": { "workspace": "ws_snapshot", "target": task_id.clone(), "max_chars": 4000, "graph_diff": true } }
    }));
    let snapshot_text = extract_tool_text(&snapshot);
    assert_eq!(
        snapshot_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(
        snapshot_text
            .get("result")
            .and_then(|v| v.get("capsule"))
            .is_some(),
        "snapshot must include capsule for handoff"
    );
    assert!(
        snapshot_text
            .get("result")
            .and_then(|v| v.get("graph_diff"))
            .is_some(),
        "snapshot must include graph_diff payload"
    );

    let macro_note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "macro_branch_note", "arguments": { "workspace": "ws_snapshot", "name": "initiative/smoke", "content": "hello" } }
    }));
    assert!(
        !extract_tool_text_str(&macro_note).starts_with("ERROR:"),
        "macro_branch_note portal must succeed"
    );
}
