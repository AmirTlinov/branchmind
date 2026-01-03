#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_macro_flow_smoke() {
    let mut server = Server::start_initialized("tasks_macro_flow_smoke");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Macro",
                "task_title": "Task Macro",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start must succeed"
    );
    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws1" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus task id")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": {
                "workspace": "ws1",
                "checkpoints": "gate",
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&close).starts_with("ERROR:"),
        "macro_close_step must succeed"
    );

    let finish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_finish",
            "arguments": {
                "workspace": "ws1",
                "task": task_id
            }
        }
    }));
    let finish_text = extract_tool_text(&finish);
    assert!(
        finish_text
            .get("result")
            .and_then(|v| v.get("handoff"))
            .is_some()
    );
}
