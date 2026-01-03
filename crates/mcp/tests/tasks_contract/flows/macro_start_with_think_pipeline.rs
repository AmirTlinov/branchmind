#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_macro_start_with_think_pipeline() {
    let mut server = Server::start_initialized("tasks_macro_start_with_think_pipeline");

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_macro_think",
                "plan_title": "Plan Macro Think",
                "task_title": "Task Macro Think",
                "template": "basic-task",
                "think": {
                    "frame": "Macro frame",
                    "decision": "Macro decision"
                },
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start portal must succeed"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws_macro_think" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus")
        .to_string();

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": { "workspace": "ws_macro_think", "task": task_id, "max_chars": 8000 } }
    }));
    let resume_text = extract_tool_text(&resume);
    assert_eq!(
        resume_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let cards = resume_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("resume_super memory.cards");
    assert!(cards.len() >= 2, "think pipeline should create >= 2 cards");

    let notes = resume_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("resume_super memory.notes.entries");
    let has_decision_note = notes.iter().any(|n| {
        n.get("meta")
            .and_then(|v| v.get("source"))
            .and_then(|v| v.as_str())
            == Some("think_pipeline")
    });
    assert!(
        has_decision_note,
        "think pipeline should append a decision note"
    );
}
