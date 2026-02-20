#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn recovery_ux_daily_replaces_hidden_suggestions_with_portal() {
    let mut server = Server::start_initialized_with_args(
        "recovery_ux_daily_replaces_hidden_suggestions_with_portal",
        &["--toolset", "daily"],
    );

    // Portals are context-first (BM-L1 lines), so for structured ids we use explicit create tools.
    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_recovery_daily", "kind": "plan", "title": "Plan Recovery Daily" } } }
    }));
    let plan_text = extract_tool_text(&plan);
    let plan_id = plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create plan result.id")
        .to_string();

    let task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
                "workspace": "ws_recovery_daily",
                "kind": "task",
                "parent": plan_id,
                "title": "Task Recovery Daily",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            } } }
    }));
    let task_text = extract_tool_text(&task);
    let task_id = task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create task result.id")
        .to_string();
    let step_id = task_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|steps| steps.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create task result.steps[0].step_id")
        .to_string();

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.done", "args": {
                "workspace": "ws_recovery_daily",
                "task": task_id,
                "step_id": step_id
            } } }
    }));
    let done_text = extract_tool_text(&done);

    assert_eq!(
        done_text.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        done_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("CHECKPOINTS_NOT_CONFIRMED")
    );

    let actions = done_text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("error must include actions");
    assert_eq!(
        actions.len(),
        1,
        "daily toolset should return one portal recovery action"
    );
    assert_eq!(
        actions[0].get("tool").and_then(|v| v.as_str()),
        Some("tasks")
    );
    assert_eq!(
        actions[0]
            .get("args")
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("tasks.macro.close.step")
    );
    assert!(
        !actions.iter().any(|a| {
            a.get("args")
                .and_then(|v| v.get("cmd"))
                .and_then(|v| v.as_str())
                == Some("tasks.verify")
        }),
        "low-level cmd suggestions must be replaced, not duplicated"
    );
}

#[test]
fn recovery_ux_core_adds_progressive_disclosure_for_daily_portal_recovery() {
    let mut server = Server::start_initialized_with_args(
        "recovery_ux_core_adds_progressive_disclosure_for_daily_portal_recovery",
        &["--toolset", "core"],
    );

    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_recovery_core", "kind": "plan", "title": "Plan Recovery Core" } } }
    }));
    let plan_text = extract_tool_text(&plan);
    let plan_id = plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create plan result.id")
        .to_string();

    let task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
                "workspace": "ws_recovery_core",
                "kind": "task",
                "parent": plan_id,
                "title": "Task Recovery Core",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            } } }
    }));
    let task_text = extract_tool_text(&task);
    let task_id = task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create task result.id")
        .to_string();
    let step_id = task_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|steps| steps.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create task result.steps[0].step_id")
        .to_string();

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.done", "args": {
                "workspace": "ws_recovery_core",
                "task": task_id,
                "step_id": step_id
            } } }
    }));
    let done_text = extract_tool_text(&done);

    assert_eq!(
        done_text.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        done_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("CHECKPOINTS_NOT_CONFIRMED")
    );

    let actions = done_text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("error must include actions");

    assert_eq!(
        actions.len(),
        1,
        "core toolset should provide a single portal recovery action"
    );
    assert_eq!(
        actions[0].get("tool").and_then(|v| v.as_str()),
        Some("tasks")
    );
    assert_eq!(
        actions[0]
            .get("args")
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("tasks.macro.close.step")
    );
    assert!(
        !actions.iter().any(|a| {
            a.get("args")
                .and_then(|v| v.get("cmd"))
                .and_then(|v| v.as_str())
                == Some("tasks.verify")
        }),
        "low-level cmd suggestions must be replaced, not duplicated"
    );
}
