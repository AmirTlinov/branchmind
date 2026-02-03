#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_bootstrap_with_think_pipeline() {
    let mut server = Server::start_initialized("tasks_bootstrap_with_think_pipeline");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Think",
                "task_title": "Task Think",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "think": {
                    "frame": "Bootstrap frame",
                    "decision": "Bootstrap decision"
                }
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    assert_eq!(
        bootstrap_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pipeline = bootstrap_text
        .get("result")
        .and_then(|v| v.get("think_pipeline"))
        .expect("think_pipeline");
    let cards = pipeline
        .get("cards")
        .and_then(|v| v.as_array())
        .expect("think_pipeline.cards");
    assert!(cards.len() >= 2);
    assert!(pipeline.get("decision_note").is_some());
}
