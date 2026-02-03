#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_close_step_smoke() {
    let mut server = Server::start_initialized("tasks_close_step_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_close", "kind": "plan", "title": "Plan A" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_close", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.decompose", "args": { "workspace": "ws_close", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let step_id = decompose_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.step.close", "args": { "workspace": "ws_close", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": "all" } } }
    }));
    let close_text = extract_tool_text(&close);
    assert_eq!(
        close_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let events = close_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 2);
    assert_eq!(
        events
            .first()
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("step_verified")
    );
    assert_eq!(
        events
            .last()
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("step_done")
    );
}
