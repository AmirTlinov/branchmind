#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn tasks_snapshot_state_line_surfaces_cockpit_card_ref_when_available() {
    let mut server = Server::start_initialized("tasks_snapshot_state_line_surfaces_cockpit_ref");

    let delegate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.delegate", "args": {
                "workspace": "ws_primary_ref",
                "plan_title": "Plan Primary Ref",
                "task_title": "Storage: investigate slow queries",
                "description": "Goal: pinpoint the slow path and propose a minimal fix with evidence.",
                "resume_max_chars": 4000
            } } }
    }));

    let out = extract_tool_text_str(&delegate);
    assert!(
        !out.starts_with("ERROR:"),
        "tasks_macro_delegate must succeed, got: {out}"
    );

    let delegate_state = out.lines().next().unwrap_or("");
    let cockpit_id = parse_state_ref_id(delegate_state)
        .expect("expected a stable ref=... in tasks_macro_delegate state line");
    assert!(
        cockpit_id.starts_with("CARD-"),
        "expected CARD- id, got: {cockpit_id}"
    );

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": {
                "workspace": "ws_primary_ref",
                "max_chars": 4000,
                "fmt": "lines"
            } } }
    }));

    let text = extract_tool_text_str(&snapshot);
    let state = text.lines().next().unwrap_or("");
    let ref_id = parse_state_ref_id(state).unwrap_or_else(|| {
        let preview = text.lines().take(8).collect::<Vec<_>>().join("\n");
        panic!("expected ref=... in the state line.\nstate={state}\npreview:\n{preview}");
    });
    assert_eq!(
        ref_id, cockpit_id,
        "expected state-line ref to point at the cockpit card for navigation"
    );
}
