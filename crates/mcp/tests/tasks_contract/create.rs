#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn tasks_create_with_steps_sets_fields() {
    let mut server = Server::start_initialized("tasks_create_steps");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_steps", "kind": "plan", "title": "Plan Steps" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": {
            "workspace": "ws_steps",
            "kind": "task",
            "parent": plan_id.clone(),
            "title": "Task Steps",
            "steps": [
                {
                    "title": "Step A",
                    "success_criteria": ["c1"],
                    "tests": ["t1"],
                    "blockers": ["b1"]
                }
            ]
        } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume", "arguments": { "workspace": "ws_steps", "task": task_id.clone() } }
    }));
    let resume_text = extract_tool_text(&resume);
    let steps = resume_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    let step = steps.first().expect("step");
    let tests = step.get("tests").and_then(|v| v.as_array()).expect("tests");
    assert!(
        tests.iter().any(|v| v.as_str() == Some("t1")),
        "tests should include t1"
    );
    let blockers = step
        .get("blockers")
        .and_then(|v| v.as_array())
        .expect("blockers");
    assert!(
        blockers.iter().any(|v| v.as_str() == Some("b1")),
        "blockers should include b1"
    );
}

#[test]
fn tasks_templates_list_smoke() {
    let mut server = Server::start_initialized("tasks_templates_list");

    let list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_templates_list", "arguments": { "workspace": "ws_templates" } }
    }));
    let list_text = extract_tool_text(&list);
    let templates = list_text
        .get("result")
        .and_then(|v| v.get("templates"))
        .and_then(|v| v.as_array())
        .expect("templates");
    assert!(
        templates
            .iter()
            .any(|t| t.get("id").and_then(|v| v.as_str()) == Some("basic-task")),
        "templates_list should include basic-task"
    );
}

#[test]
fn tasks_scaffold_task_smoke() {
    let mut server = Server::start_initialized("tasks_scaffold");

    let scaffold = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_scaffold",
            "arguments": {
                "workspace": "ws_scaffold",
                "template": "basic-task",
                "kind": "task",
                "title": "Scaffold Task",
                "plan_title": "Scaffold Plan"
            }
        }
    }));
    let scaffold_text = extract_tool_text(&scaffold);
    assert_eq!(
        scaffold_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let steps = scaffold_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("steps");
    assert!(!steps.is_empty(), "scaffold should create steps");
}
