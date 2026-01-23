#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_bootstrap_allows_steps_without_tests() {
    let mut server = Server::start_initialized("tasks_bootstrap_allows_steps_without_tests");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_optional_tests",
                "plan_title": "Plan Optional Tests",
                "task_title": "Task Optional Tests",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    assert_eq!(
        bootstrap_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn tasks_macro_start_allows_steps_without_tests() {
    let mut server = Server::start_initialized("tasks_macro_start_allows_steps_without_tests");

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_macro_optional_tests",
                "plan_title": "Plan Macro Optional Tests",
                "task_title": "Task Macro Optional Tests",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"] }
                ],
                "resume_max_chars": 2000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start portal must succeed"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_focus_get",
            "arguments": { "workspace": "ws_macro_optional_tests" }
        }
    }));
    let focus_text = extract_tool_text(&focus);
    let focus_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus");
    assert!(focus_id.starts_with("TASK-"), "focus must point to a task");
}
