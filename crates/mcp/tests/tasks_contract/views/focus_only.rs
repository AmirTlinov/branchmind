#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_snapshot_focus_only_returns_step_focus_and_minimized_memory() {
    let mut server = Server::start_initialized("tasks_snapshot_focus_only_returns_step_focus");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_focus_only",
                "plan_title": "Plan Focus",
                "task_title": "Task Focus",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": ["b1"] }
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

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": { "workspace": "ws_focus_only", "task": task_id, "view": "focus_only", "max_chars": 4000 }
        }
    }));
    let resume_text = extract_tool_text(&resume);

    let result = resume_text.get("result").expect("result");
    assert!(
        result.get("step_focus").is_some(),
        "focus_only should include step_focus"
    );
    assert!(
        result.get("graph_diff").is_none(),
        "focus_only should not include graph_diff by default"
    );

    let notes_len = result
        .get("memory")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let trace_len = result
        .get("memory")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    assert_eq!(notes_len, 0, "focus_only should minimize notes");
    assert_eq!(trace_len, 0, "focus_only should minimize trace");
}
