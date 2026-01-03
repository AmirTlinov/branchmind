#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_think_pipeline_smoke() {
    let mut server = Server::start_initialized("branchmind_think_pipeline_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think_pipe", "kind": "plan", "title": "Plan Pipe" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_think_pipe", "kind": "task", "parent": plan_id, "title": "Task Pipe" } }
    }));
    let task_id = extract_tool_text(&created_task)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let pipeline = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_pipeline",
            "arguments": {
                "workspace": "ws_think_pipe",
                "target": task_id,
                "frame": "Frame",
                "hypothesis": "Hypothesis",
                "test": "Test",
                "evidence": "Evidence",
                "decision": "Decision"
            }
        }
    }));
    let pipeline_text = extract_tool_text(&pipeline);
    let cards = pipeline_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert_eq!(cards.len(), 5);
    let decision_note = pipeline_text
        .get("result")
        .and_then(|v| v.get("decision_note"))
        .expect("decision_note");
    assert!(decision_note.get("card_id").is_some());
}
