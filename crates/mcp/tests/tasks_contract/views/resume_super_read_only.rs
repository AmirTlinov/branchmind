#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_resume_super_read_only_smoke() {
    let mut server = Server::start_initialized("tasks_resume_super_read_only_smoke");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Super",
                "task_title": "Task Super",
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
        .expect("task id")
        .to_string();
    let plan_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.set", "args": { "workspace": "ws1", "task": plan_id.clone() } } }
    }));

    let resume_super = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": { "workspace": "ws1", "task": task_id.clone(), "read_only": true, "max_chars": 4000 } } }
    }));
    let resume_text = extract_tool_text(&resume_super);
    assert!(
        resume_text
            .get("result")
            .and_then(|v| v.get("memory"))
            .is_some()
    );
    assert!(
        resume_text
            .get("result")
            .and_then(|v| v.get("degradation"))
            .is_some()
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.focus.get", "args": { "workspace": "ws1" } } }
    }));
    let focus_text = extract_tool_text(&focus);
    assert_eq!(
        focus_text
            .get("result")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str()),
        Some(plan_id.as_str())
    );
}
