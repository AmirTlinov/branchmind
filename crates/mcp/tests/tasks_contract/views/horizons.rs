#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_snapshot_plan_includes_horizon_counts_without_listing_backlog() {
    let mut server = Server::start_initialized("tasks_snapshot_plan_includes_horizon_counts");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_horizons", "kind": "plan", "title": "Plan H" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_horizons", "kind": "task", "parent": plan_id.clone(), "title": "Task 1" } } }
    }));
    let task1_id = extract_tool_text(&task1)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task 1 id")
        .to_string();

    let _task2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_horizons", "kind": "task", "parent": plan_id.clone(), "title": "Task 2" } } }
    }));
    let _task3 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_horizons", "kind": "task", "parent": plan_id.clone(), "title": "Task 3" } } }
    }));

    // Promote one task into the active horizon.
    let _activate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.complete", "args": { "workspace": "ws_horizons", "task": task1_id, "status": "ACTIVE" } } }
    }));

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "workspace": "ws_horizons", "plan": plan_id, "max_chars": 2000, "fmt": "lines" } } }
    }));
    let text = extract_tool_text_str(&snapshot);

    assert!(
        text.contains("horizon active=1 backlog=2 parked=0 stale=0 done=0 total=3"),
        "plan snapshot should show horizon counts in the state line"
    );
    assert!(
        !text.contains("Task 2") && !text.contains("Task 3"),
        "backlog should not be listed by default (counts only)"
    );
}
