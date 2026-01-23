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
fn flagship_eval_snapshot_is_two_lines_and_ref_first() {
    let mut server = Server::start_initialized_with_args(
        "flagship_eval_snapshot_is_two_lines_and_ref_first",
        &["--toolset", "daily", "--workspace", "ws_flagship_eval"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Flagship Eval Task" } }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": {} }
    }));
    let text = extract_tool_text_str(&snapshot);
    assert_tag_light(&text);

    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        2,
        "flagship BM-L1 snapshot must be 2 lines (state + command), got {} lines:\n{text}",
        lines.len()
    );

    let state = lines[0];
    assert!(
        state.contains("focus TASK-") && state.contains("| where="),
        "state line must include focus + where=..."
    );
    let ref_id = parse_state_ref_id(state).expect("state line must include ref=");
    assert!(
        ref_id.starts_with("CARD-") || ref_id.starts_with("TASK-") || ref_id.starts_with("PLAN-"),
        "ref must be an openable id, got: {ref_id}"
    );

    assert!(
        lines[1].starts_with("think_card"),
        "when where=unknown, second line must be the canonical anchor attach command"
    );
    assert!(
        lines[1].contains("v:canon"),
        "anchor attach suggestion must be canonical (v:canon)"
    );
    assert!(
        !lines[1].contains("workspace="),
        "when default workspace is configured, command line must omit workspace"
    );
    assert!(
        state.contains("| backup tasks_macro_close_step"),
        "state line must preserve the progress action as a backup command"
    );
}

#[test]
fn flagship_eval_snapshot_ref_survives_max_chars_2000() {
    let mut server = Server::start_initialized_with_args(
        "flagship_eval_snapshot_ref_survives_max_chars_2000",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flagship_eval_budget",
        ],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Flagship Eval Budget Task" } }
    }));

    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_note",
            "arguments": {
                "path": "s:0",
                "note": "x".repeat(40_000)
            }
        }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": { "max_chars": 2000 } }
    }));
    let text = extract_tool_text_str(&snapshot);
    assert_tag_light(&text);
    let state = text.lines().next().unwrap_or("");
    let ref_id = parse_state_ref_id(state).expect("ref must survive max_chars trimming");
    assert!(
        ref_id.starts_with("CARD-") || ref_id.starts_with("TASK-") || ref_id.starts_with("PLAN-"),
        "ref must be an openable id under trimming, got: {ref_id}"
    );
}

#[test]
fn flagship_eval_resume_super_capsule_survives_max_chars_2000() {
    let mut server = Server::start_initialized("flagship_eval_resume_super_capsule_survives");

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "workspace": "ws_flagship_eval_capsule", "task_title": "Flagship Eval Capsule Task" } }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_flagship_eval_capsule",
                "view": "smart",
                "max_chars": 2000
            }
        }
    }));
    let parsed = extract_tool_text(&resume);
    let capsule = parsed
        .get("result")
        .and_then(|v| v.get("capsule"))
        .cloned()
        .unwrap_or_default();
    assert!(
        capsule.is_object(),
        "resume_super must include capsule under tight budgets, got: {parsed}"
    );
    assert!(
        capsule
            .get("action")
            .and_then(|v| v.get("tool"))
            .and_then(|v| v.as_str())
            .is_some(),
        "capsule.action.tool must be present for deterministic next action"
    );

    if let Some(actions) = parsed
        .get("result")
        .and_then(|v| v.get("engine"))
        .and_then(|v| v.get("actions"))
        .and_then(|v| v.as_array())
    {
        assert!(
            actions.len() <= 2,
            "engine actions must stay bounded (primary + backup), got {} actions",
            actions.len()
        );
    }
}
