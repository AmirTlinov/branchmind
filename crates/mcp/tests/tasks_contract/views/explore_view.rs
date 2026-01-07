#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_resume_super_explore_warms_archive_vs_smart() {
    let mut server = Server::start_initialized("tasks_resume_super_explore_warms_archive");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_explore_view",
                "plan_title": "Plan Explore",
                "task_title": "Task Explore",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
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

    let _card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_explore_view",
            "target": task_id.clone(),
            "card": { "id": "CARD-ARCH", "type": "hypothesis", "title": "Archived", "text": "closed hypothesis" }
        } }
    }));
    let _closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_set_status", "arguments": {
            "workspace": "ws_explore_view",
            "target": task_id.clone(),
            "status": "closed",
            "targets": ["CARD-ARCH"]
        } }
    }));

    let smart = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": {
            "workspace": "ws_explore_view",
            "task": task_id.clone(),
            "view": "smart",
            "cards_limit": 20,
            "max_chars": 8000
        } }
    }));
    let smart_text = extract_tool_text(&smart);
    let smart_cards = smart_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("memory.cards");
    assert!(
        smart_cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-ARCH")),
        "smart view should keep the archive cold by default"
    );

    let explore = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": {
            "workspace": "ws_explore_view",
            "task": task_id,
            "view": "explore",
            "cards_limit": 20,
            "max_chars": 8000
        } }
    }));
    let explore_text = extract_tool_text(&explore);
    let explore_cards = explore_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("memory.cards");
    assert!(
        explore_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-ARCH")),
        "explore view should allow warm archive padding"
    );
}
