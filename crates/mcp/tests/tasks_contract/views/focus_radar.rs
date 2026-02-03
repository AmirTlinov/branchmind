#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::Value;
use serde_json::json;

#[test]
fn tasks_focus_and_radar_smoke() {
    let mut server = Server::start_initialized("tasks_focus_and_radar_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let focus_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": { "workspace": "ws1" } } }
    }));
    let focus_before_text = extract_tool_text(&focus_before);
    assert_eq!(
        focus_before_text.get("result").and_then(|v| v.get("focus")),
        Some(&Value::Null)
    );

    let radar_without_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws1" } } }
    }));
    assert_eq!(
        radar_without_focus
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let radar_without_focus_text = extract_tool_text(&radar_without_focus);
    assert_eq!(
        radar_without_focus_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
    let actions = radar_without_focus_text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions");
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("tasks")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("tasks.snapshot")
        }),
        "tasks_radar without focus must include a tasks.snapshot action"
    );

    let focus_set = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.set", "args": { "workspace": "ws1", "task": task_id.clone() } } }
    }));
    let focus_set_text = extract_tool_text(&focus_set);
    assert_eq!(
        focus_set_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );

    let radar_from_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws1", "max_chars": 10 } } }
    }));
    assert_eq!(
        radar_from_focus
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    let radar_from_focus_text = extract_tool_text(&radar_from_focus);
    assert_eq!(
        radar_from_focus_text
            .get("result")
            .and_then(|v| v.get("budget"))
            .and_then(|v| v.get("max_chars"))
            .and_then(|v| v.as_u64()),
        Some(10)
    );

    let radar_full = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws1", "max_chars": 400 } } }
    }));
    let radar_full_text = extract_tool_text(&radar_full);
    let expected_branch = format!("task/{task_id}");
    assert_eq!(
        radar_full_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert_eq!(
        radar_full_text
            .get("result")
            .and_then(|v| v.get("reasoning_ref"))
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(expected_branch.as_str())
    );

    let focus_cleared = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.clear", "args": { "workspace": "ws1" } } }
    }));
    let focus_cleared_text = extract_tool_text(&focus_cleared);
    assert_eq!(
        focus_cleared_text
            .get("result")
            .and_then(|v| v.get("cleared"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}
