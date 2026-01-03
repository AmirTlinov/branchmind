#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_define_normalizes_blockers() {
    let mut server = Server::start_initialized("tasks_define_normalizes_blockers");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Normalize",
                "task_title": "Task Normalize",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step_path = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .expect("step path")
        .to_string();

    let defined = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_define", "arguments": { "workspace": "ws1", "task": task_id.clone(), "path": step_path, "blockers": ["None"] } }
    }));
    let defined_text = extract_tool_text(&defined);
    assert_eq!(
        defined_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));
    let resume_text = extract_tool_text(&resume);
    let steps = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    let blockers = steps
        .iter()
        .find(|s| s.get("step_id").is_some())
        .and_then(|s| s.get("blockers"))
        .and_then(|v| v.as_array())
        .expect("blockers");
    assert!(blockers.is_empty());
}
