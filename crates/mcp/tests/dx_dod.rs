#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn assert_tag_light(text: &str) {
    assert!(
        !text.trim_start().starts_with('{'),
        "fmt=lines must not fall back to JSON envelopes"
    );
    assert!(
        !text.contains("WATERMARK:") && !text.contains("ANSWER:"),
        "fmt=lines must not include legacy tag prefixes for content lines"
    );
    assert!(
        !text.contains("\n\n"),
        "fmt=lines must not include empty lines"
    );
    for (idx, line) in text.lines().enumerate() {
        assert!(
            !line.trim().is_empty(),
            "fmt=lines must not include empty line at {idx}"
        );
    }
}

#[test]
fn dx_dod_daily_status_is_state_plus_command() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_daily_status_is_state_plus_command",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_status"],
    );

    let status = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "status", "arguments": {} }
    }));
    let text = extract_tool_text_str(&status);
    assert_tag_light(&text);

    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        2,
        "daily status must be 2 lines (state + command)"
    );
    assert!(
        lines[0].starts_with("ready checkout="),
        "first line should be a stable state summary"
    );
    assert!(
        lines[1].starts_with("tasks_snapshot"),
        "second line should be a low-noise next action"
    );
}

#[test]
fn dx_dod_daily_task_flow_is_state_plus_command() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_daily_task_flow_is_state_plus_command",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_flow"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "DX DoD Task" } }
    }));
    let start_text = extract_tool_text_str(&started);
    assert_tag_light(&start_text);
    let start_lines = start_text.lines().collect::<Vec<_>>();
    assert_eq!(
        start_lines.len(),
        2,
        "daily start must be 2 lines (state + command)"
    );
    assert!(
        start_lines[0].contains("focus TASK-") && start_lines[0].contains("| next gate"),
        "start state line should include focus + next gate hint"
    );
    assert!(
        start_lines[1].starts_with("tasks_macro_close_step"),
        "start should suggest the next action as a command"
    );
    assert!(
        !start_lines[1].contains("workspace="),
        "when default workspace is configured, action commands should omit workspace"
    );

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": {} }
    }));
    let snap_text = extract_tool_text_str(&snapshot);
    assert_tag_light(&snap_text);
    let snap_lines = snap_text.lines().collect::<Vec<_>>();
    assert_eq!(
        snap_lines.len(),
        2,
        "daily snapshot must be 2 lines (state + command)"
    );
    assert!(
        snap_lines[0].contains("focus TASK-") && snap_lines[0].contains("| next gate"),
        "snapshot state line should include focus + next gate hint"
    );
    assert!(
        snap_lines[1].starts_with("tasks_macro_close_step"),
        "snapshot should suggest the next action as a command"
    );
    assert!(
        !snap_lines[1].contains("workspace="),
        "when default workspace is configured, action commands should omit workspace"
    );
}

#[test]
fn dx_dod_no_focus_recovery_is_typed_and_portal_first() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_no_focus_recovery_is_typed_and_portal_first",
        &["--toolset", "daily"],
    );

    let close = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "workspace": "ws_dx_dod_no_focus", "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&close);
    assert_tag_light(&text);
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "no-focus recovery must stay 2 lines");
    assert!(
        lines[0].starts_with("ERROR: INVALID_INPUT"),
        "no-focus recovery must be a typed error"
    );
    assert!(
        lines[1].starts_with("tasks_macro_start"),
        "no-focus recovery must suggest the portal macro_start (no full disclosure)"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "no-focus recovery must not force toolset disclosure"
    );
}

#[test]
fn dx_dod_progressive_disclosure_is_two_commands_only() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_progressive_disclosure_is_two_commands_only",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_disclosure"],
    );

    // Create a task with no steps to force the capsule to recommend tasks_decompose (hidden).
    let created_plan = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "workspace": "ws_dx_dod_disclosure", "kind": "plan", "title": "Plan" }
        }
    }));
    let plan_json = extract_tool_text(&created_plan);
    let plan_id = plan_json
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("created_plan.id");

    let created_task = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "workspace": "ws_dx_dod_disclosure", "kind": "task", "parent": plan_id, "title": "No Steps" }
        }
    }));
    let task_json = extract_tool_text(&created_task);
    let task_id = task_json
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("created_task.id");

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": { "task": task_id, "fmt": "lines" } }
    }));
    let text = extract_tool_text_str(&snapshot);
    assert_tag_light(&text);

    let lines = text.lines().collect::<Vec<_>>();
    assert!(
        lines.len() == 3,
        "progressive disclosure should be 3 lines (state + tools/list + action), got {}",
        lines.len()
    );
    assert!(
        !lines[0].starts_with("ERROR:"),
        "first line should be the state line"
    );
    assert!(
        lines[1].starts_with("tools/list toolset=full"),
        "first command must be toolset disclosure"
    );
    assert!(
        lines[2].starts_with("tasks_decompose"),
        "second command must be the hidden action"
    );
    assert!(
        lines[2].contains("task=") && lines[2].contains("steps="),
        "hidden action must include args_hint for copy/paste"
    );
}

#[test]
fn dx_dod_budget_warnings_remain_warnings_and_stay_small() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_budget_warnings_remain_warnings_and_stay_small",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_budget"],
    );

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Budget Task" } }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": { "max_chars": 50, "fmt": "lines" } }
    }));
    let text = extract_tool_text_str(&snapshot);
    assert_tag_light(&text);
    assert!(
        text.contains("WARNING: BUDGET_"),
        "budget truncation must be surfaced as WARNING lines"
    );
    assert!(
        !text.contains("ERROR: BUDGET_"),
        "budget warnings must never be rendered as errors"
    );
    let line_count = text.lines().count();
    assert!(
        line_count <= 4,
        "budget warnings must remain small (<=4 lines), got {line_count}"
    );
}

#[test]
fn dx_dod_more_is_copy_paste_ready_when_no_action() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_more_is_copy_paste_ready_when_no_action",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_more"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "DX More Task" } }
    }));

    // Create enough notes to force paging (notes_limit defaults to 10).
    for i in 0..15 {
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + i,
            "method": "tools/call",
            "params": {
                "name": "tasks_note",
                "arguments": {
                    "workspace": "ws_dx_dod_more",
                    "path": "s:0",
                    "note": format!("note {i}")
                }
            }
        }));
    }

    // Finish the task so the capsule has no "next action" command to suggest.
    for i in 0..4 {
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 40 + i,
            "method": "tools/call",
            "params": { "name": "tasks_macro_close_step", "arguments": {} }
        }));
    }

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": {} }
    }));
    let text = extract_tool_text_str(&snapshot);
    assert_tag_light(&text);

    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "continuation should stay 2 lines");
    assert!(
        lines[1].starts_with("tasks_snapshot"),
        "continuation must be a copy/paste-ready snapshot command"
    );
    assert!(
        lines[1].contains("notes_cursor="),
        "continuation command must include notes_cursor"
    );
    assert!(
        !lines[1].contains("workspace="),
        "when default workspace is configured, continuation commands should omit workspace"
    );
    assert!(
        !text.contains("MORE:"),
        "continuation should not require decoding a MORE cursor line"
    );
}

#[test]
fn dx_dod_done_state_does_not_emit_already_done_warning() {
    let mut server = Server::start_initialized_with_args(
        "dx_dod_done_state_does_not_emit_already_done_warning",
        &["--toolset", "daily", "--workspace", "ws_dx_dod_done"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "DX Done Task" } }
    }));

    // Finish the basic-task template (3 steps + finish).
    for i in 0..4 {
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + i,
            "method": "tools/call",
            "params": { "name": "tasks_macro_close_step", "arguments": {} }
        }));
    }

    // Calling the progress macro again should stay quiet: DONE is a state, not a warning.
    let close_again = server.request(json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": { "name": "tasks_macro_close_step", "arguments": {} }
    }));
    let text = extract_tool_text_str(&close_again);
    assert_tag_light(&text);

    let first = text.lines().next().unwrap_or("");
    assert!(
        first.contains("| done"),
        "DONE state must be explicit in the state line"
    );
    assert!(
        !text.contains("ALREADY_DONE"),
        "DONE state must not emit noisy ALREADY_DONE warnings"
    );
}
