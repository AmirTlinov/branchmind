#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_bootstrap_and_resume_pack_smoke() {
    let mut server = Server::start_initialized("tasks_bootstrap_and_resume_pack");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Bootstrap",
                "task_title": "Task Bootstrap",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] },
                    { "title": "S2", "success_criteria": ["c2"], "tests": ["t2"] }
                ]
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let steps = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    assert_eq!(steps.len(), 2);

    let resume_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.pack", "args": { "workspace": "ws1", "task": task_id.clone(), "events_limit": 10, "max_chars": 2000 } } }
    }));
    let resume_pack_text = extract_tool_text(&resume_pack);
    assert_eq!(
        resume_pack_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert!(
        resume_pack_text
            .get("result")
            .and_then(|v| v.get("radar"))
            .is_some()
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": { "workspace": "ws1" } } }
    }));
    let focus_text = extract_tool_text(&focus);
    assert_eq!(
        focus_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
}
