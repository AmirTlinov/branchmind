#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_macro_delegate_smoke() {
    let mut server = Server::start_initialized("tasks_macro_delegate_smoke");

    let delegate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_delegate",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Delegate",
                "task_title": "Storage: investigate slow queries",
                "description": "Goal: pinpoint the slow path and propose a minimal fix with evidence.",
                "resume_max_chars": 4000
            }
        }
    }));
    let out = extract_tool_text_str(&delegate);
    assert!(
        !out.starts_with("ERROR:"),
        "tasks_macro_delegate must succeed, got: {out}"
    );

    let state = out.lines().next().unwrap_or("");
    let cockpit_id =
        parse_state_ref_id(state).expect("expected ref=... in tasks_macro_delegate state line");
    assert!(
        cockpit_id.starts_with("CARD-"),
        "expected CARD- id, got: {cockpit_id}"
    );

    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "open",
            "arguments": {
                "workspace": "ws1",
                "id": cockpit_id,
                "max_chars": 4000
            }
        }
    }));
    let opened = extract_tool_text(&open);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(cockpit) must succeed: {opened}"
    );

    let card = opened
        .get("result")
        .and_then(|v| v.get("card"))
        .and_then(|v| v.as_object())
        .expect("open must return kind=card with card object");
    assert_eq!(
        card.get("type").and_then(|v| v.as_str()).unwrap_or("-"),
        "frame",
        "cockpit must be a frame card"
    );
    let tags = card
        .get("tags")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let tags: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
    assert!(tags.contains(&"pinned"), "cockpit must be pinned");
    assert!(tags.contains(&"v:canon"), "cockpit must be canon-visible");
    assert!(
        tags.contains(&"a:storage"),
        "expected derived anchor a:storage, tags={tags:?}"
    );
}
