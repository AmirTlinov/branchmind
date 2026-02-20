#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn tasks_snapshot_truncation_inserts_stable_reference_line_second() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_truncation_inserts_stable_reference_line_second",
        &["--workspace", "ws_snapshot_ref_line"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Ref Line Task" } } }
    }));

    // Force truncation deterministically so the snapshot must surface a stable reference line
    // right after the state line.
    let huge_note = "n".repeat(40_000);
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.note", "args": {
                "workspace": "ws_snapshot_ref_line",
                "path": "s:0",
                "note": huge_note
            } } }
    }));
    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "max_chars": 2000, "fmt": "lines" } } }
    }));
    let text = extract_tool_text_str(&snapshot);

    let lines = text.lines().collect::<Vec<_>>();
    assert!(
        lines.len() >= 2,
        "expected at least 2 lines (state + command), got {}:\n{text}",
        lines.len()
    );
    let state = lines[0];
    let ref_id = parse_state_ref_id(state)
        .unwrap_or_else(|| panic!("expected ref=... in state line under truncation, got: {state}"));
    assert!(
        ref_id.starts_with("CARD-") || ref_id.starts_with("TASK-") || ref_id.starts_with("PLAN-"),
        "state-line ref must be an openable id, got: {ref_id}"
    );
}

#[test]
fn tasks_snapshot_truncation_prefers_cockpit_card_reference() {
    let mut server = Server::start_initialized("tasks_snapshot_truncation_prefers_cockpit_ref");

    let delegated = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.delegate", "args": {
                "workspace": "ws_snapshot_cockpit_ref",
                "plan_title": "Plan Cockpit Ref",
                "task_title": "Delegated Task (cockpit ref)"
            } } }
    }));
    let delegated_text = extract_tool_text_str(&delegated);
    let delegate_state = delegated_text.lines().next().unwrap_or("");
    let cockpit_id = parse_state_ref_id(delegate_state)
        .expect("expected ref=... in tasks_macro_delegate state line");
    assert!(
        cockpit_id.starts_with("CARD-"),
        "expected cockpit id to be CARD-*, got: {cockpit_id}"
    );

    // Ensure we trigger truncation without relying on implicit budgets.
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.note", "args": {
                "workspace": "ws_snapshot_cockpit_ref",
                "path": "s:0",
                "note": "x".repeat(20_000)
            } } }
    }));

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": {
                "workspace": "ws_snapshot_cockpit_ref",
                "max_chars": 2000,
                "fmt": "lines"
            } } }
    }));
    let text = extract_tool_text_str(&snapshot);
    let lines = text.lines().collect::<Vec<_>>();
    assert!(
        lines.len() >= 2,
        "expected at least 2 lines (state + command), got {}:\n{text}",
        lines.len()
    );
    let state = lines[0];
    let ref_id = parse_state_ref_id(state)
        .unwrap_or_else(|| panic!("expected ref=... in state line, got: {state}"));
    assert_eq!(
        ref_id, cockpit_id,
        "expected state-line ref to prefer the pinned cockpit card id"
    );
}
