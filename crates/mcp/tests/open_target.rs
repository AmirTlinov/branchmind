#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

fn focus_id_from_portal_line(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    assert!(
        first.starts_with("focus "),
        "expected a portal state line starting with `focus ...`, got: {first}"
    );
    first
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[test]
fn open_task_is_supported_and_is_read_only() {
    let mut server = Server::start_initialized_with_args(
        "open_task_is_supported_and_is_read_only",
        &["--toolset", "daily", "--workspace", "ws_open_task"],
    );

    let started1 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Open Task 1" } } }
    }));
    let task1 = focus_id_from_portal_line(&extract_tool_text_str(&started1));
    assert!(task1.starts_with("TASK-"), "expected a TASK-* id");

    let started2 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Open Task 2" } } }
    }));
    let task2 = focus_id_from_portal_line(&extract_tool_text_str(&started2));
    assert!(task2.starts_with("TASK-"), "expected a TASK-* id");

    let _set_focus = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.set", "args": { "task": task1 } } }
    }));

    let focused_before = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": {} } }
    }));
    let focused_before = extract_tool_text(&focused_before);
    assert_eq!(
        focused_before
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task1.as_str()),
        "focus must be set to task1 before open"
    );

    let opened = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": task2 } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(task) should succeed"
    );
    let result = opened.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("task"),
        "open(TASK-*) must return kind=task"
    );
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(task2.as_str()),
        "open(task) must preserve id"
    );
    assert!(
        result.get("capsule").is_some(),
        "open(task) must include a capsule for navigation"
    );
    assert!(
        result.get("reasoning_ref").is_some(),
        "open(task) must include reasoning refs"
    );

    let focused_after = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": {} } }
    }));
    let focused_after = extract_tool_text(&focused_after);
    assert_eq!(
        focused_after
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task1.as_str()),
        "open(task) must not change focus (read-only)"
    );
}

#[test]
fn open_plan_is_supported_and_is_read_only() {
    let mut server = Server::start_initialized_with_args(
        "open_plan_is_supported_and_is_read_only",
        &["--toolset", "daily", "--workspace", "ws_open_plan"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Focus Task" } } }
    }));
    let focus_task = focus_id_from_portal_line(&extract_tool_text_str(&started));

    let created_plan = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_open_plan", "kind": "plan", "title": "Open Plan" } } }
    }));
    let created_plan = extract_tool_text(&created_plan);
    assert!(
        created_plan
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_create(plan) should succeed"
    );
    let plan_id = created_plan
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id");
    assert!(plan_id.starts_with("PLAN-"), "expected PLAN-* id");

    let opened = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": plan_id } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(plan) should succeed"
    );
    let result = opened.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("plan"),
        "open(PLAN-*) must return kind=plan"
    );
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(plan_id),
        "open(plan) must preserve id"
    );

    let focused_after = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": {} } }
    }));
    let focused_after = extract_tool_text(&focused_after);
    assert_eq!(
        focused_after
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(focus_task.as_str()),
        "open(plan) must not change focus (read-only)"
    );
}
