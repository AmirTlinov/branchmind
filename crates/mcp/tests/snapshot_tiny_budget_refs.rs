#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn ref_id_from_reference_line(line: &str) -> &str {
    let head = line.split('|').next().unwrap_or(line);
    let mut parts = head.split_whitespace();
    let tag = parts.next().unwrap_or("");
    assert_eq!(tag, "REFERENCE:", "expected REFERENCE line");
    let second = parts.next().unwrap_or("");
    let third = parts.next();
    // New stable snapshot refs use `REFERENCE: <id>`; legacy refs use `REFERENCE: <LABEL> <id>`.
    let id = third.unwrap_or(second);
    assert!(!id.trim().is_empty(), "reference id must not be empty");
    id
}

#[test]
fn tasks_snapshot_tiny_budget_still_emits_openable_refs_and_prefers_open_task_jump() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_tiny_budget_still_emits_openable_refs_and_prefers_open_task_jump",
        &["--workspace", "ws_tiny_budget_refs"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Tiny Budget Refs Task" } }
    }));

    let huge_note = "n".repeat(120_000);
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_note",
            "arguments": {
                "workspace": "ws_tiny_budget_refs",
                "path": "s:0",
                "note": huge_note
            }
        }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": { "max_chars": 900, "refs": true } }
    }));
    let text = extract_tool_text_str(&snapshot);
    let lines = text.lines().collect::<Vec<_>>();
    assert!(
        lines
            .get(1)
            .copied()
            .unwrap_or("")
            .starts_with("REFERENCE: "),
        "expected a stable REFERENCE line near the top under truncation, got:\n{text}"
    );
    let ref_line = lines.get(1).copied().unwrap_or("");
    let ref_id = ref_id_from_reference_line(ref_line);
    assert!(
        ref_id.starts_with("CARD-")
            || ref_id.starts_with("TASK-")
            || ref_id.starts_with("PLAN-")
            || ref_id.contains('@'),
        "expected REFERENCE to contain an openable id, got: {ref_line}"
    );

    assert!(
        text.lines()
            .any(|l| l.starts_with("WARNING: BUDGET_TRUNCATED") || l.contains("trimmed(")),
        "expected snapshot to be trimmed under a tiny max_chars budget"
    );
    let ref_pos = text
        .lines()
        .position(|l| l.starts_with("REFERENCE: "))
        .expect("expected at least one REFERENCE line under truncation");
    assert!(
        ref_pos <= 1,
        "REFERENCE line must be near the top (after the state line), got index {ref_pos}"
    );
    let ref_line = text
        .lines()
        .find(|l| l.starts_with("REFERENCE: "))
        .unwrap_or("");
    let ref_id = ref_id_from_reference_line(ref_line);
    assert!(
        ref_id.starts_with("CARD-")
            || ref_id.starts_with("TASK-")
            || ref_id.starts_with("PLAN-")
            || ref_id.contains('@'),
        "REFERENCE line must contain an openable id, got: {ref_line}"
    );

    let open_line = text
        .lines()
        .find(|l| l.starts_with("open "))
        .expect("expected an open command line under truncation");
    let open_args = parse_open_command_line(open_line);
    let open_id = open_args
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    assert!(
        open_id.starts_with("TASK-") || open_id.starts_with("PLAN-"),
        "open jump should prefer opening the target (TASK/PLAN), got: {open_id}"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": open_args }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(jump) should succeed"
    );
}
