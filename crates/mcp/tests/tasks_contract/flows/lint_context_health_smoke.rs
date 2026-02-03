#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_lint_context_health_smoke() {
    let mut server = Server::start_initialized("tasks_lint_context_health_smoke");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Lint",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id");

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.lint", "args": { "workspace": "ws1", "task": task_id } } }
    }));
    let lint_text = extract_tool_text(&lint);
    assert!(
        lint_text
            .get("result")
            .and_then(|v| v.get("context_health"))
            .is_some()
    );
}
