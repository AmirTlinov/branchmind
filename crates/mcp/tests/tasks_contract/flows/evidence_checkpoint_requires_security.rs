#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::Value;
use serde_json::json;

#[test]
fn tasks_evidence_checkpoint_requires_security_for_first_open_step() {
    let mut server = Server::start_initialized("tasks_evidence_checkpoint_requires_security");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_chk", "kind": "plan", "title": "Plan A" } } }
    }));
    let plan_id = extract_tool_text(&created_plan)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_chk", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let task_id = extract_tool_text(&created_task)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.decompose", "args": { "workspace": "ws_chk", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } } }
    }));
    let step_id = extract_tool_text(&decompose)
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.evidence.capture", "args": {
                "workspace": "ws_chk",
                "task": task_id.clone(),
                "step_id": step_id,
                "checkpoint": "security",
                "checks": ["ci:security-scan passed"]
            } } }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": { "workspace": "ws_chk", "task": task_id, "max_chars": 4000 } } }
    }));
    let resume_text = extract_tool_text(&resume);

    let first_open = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get("first_open"))
        .cloned()
        .unwrap_or(Value::Null);
    let require_security = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get("first_open"))
        .and_then(|v| v.get("require_security"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        require_security,
        "first_open.require_security must be true; first_open={first_open:?}"
    );

    let action_checkpoints = resume_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("args"))
        .and_then(|v| v.get("checkpoints"))
        .cloned()
        .expect("capsule.action.args.checkpoints");
    assert!(
        action_checkpoints
            .get("security")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "capsule action must include security=true when required and missing"
    );
}
