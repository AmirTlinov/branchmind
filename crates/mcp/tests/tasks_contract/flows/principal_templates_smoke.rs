#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_macro_start_principal_templates_smoke() {
    let mut server = Server::start_initialized("tasks_macro_start_principal_templates_smoke");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_principal_tpl",
                "plan_title": "Principal Plan",
                "plan_template": "principal-plan",
                "task_title": "Principal Task",
                "template": "principal-task",
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start portal must succeed"
    );

    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws_principal_tpl" } }
    }));
    let context_text = extract_tool_text(&context);
    assert_eq!(
        context_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let plans = context_text
        .get("result")
        .and_then(|v| v.get("plans"))
        .and_then(|v| v.as_array())
        .expect("plans");
    let plan = plans
        .iter()
        .find(|p| p.get("title").and_then(|v| v.as_str()) == Some("Principal Plan"))
        .expect("plan entry");
    let progress = plan
        .get("plan_progress")
        .and_then(|v| v.as_str())
        .unwrap_or("0/0");
    assert_ne!(
        progress, "0/0",
        "principal-plan checklist should be applied (plan_progress must not be 0/0)"
    );

    let tasks = context_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
        .expect("tasks");
    let task = tasks
        .iter()
        .find(|t| t.get("title").and_then(|v| v.as_str()) == Some("Principal Task"))
        .expect("task entry");
    let task_id = task
        .get("id")
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let steps_count = task
        .get("steps_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert_eq!(
        steps_count, 5,
        "principal-task template should create 5 steps"
    );

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_principal_tpl", "task": task_id, "max_chars": 2000 } }
    }));
    let radar_text = extract_tool_text(&radar);
    assert_eq!(
        radar_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("reasoning_mode"))
            .and_then(|v| v.as_str()),
        Some("strict"),
        "principal-task should default to strict reasoning_mode"
    );
}
