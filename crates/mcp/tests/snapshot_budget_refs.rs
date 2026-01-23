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
fn tasks_snapshot_budget_truncation_emits_openable_refs() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_budget_truncation_emits_openable_refs",
        &["--workspace", "ws_budget_refs"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Budget Refs Task" } }
    }));

    let huge_note = "n".repeat(40_000);
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_note",
            "arguments": {
                "workspace": "ws_budget_refs",
                "path": "s:0",
                "note": huge_note
            }
        }
    }));

    let huge_text = "d".repeat(60_000);
    let _decision = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_add_decision",
            "arguments": {
                "workspace": "ws_budget_refs",
                "card": {
                    "title": "Decision: huge content",
                    "text": huge_text
                }
            }
        }
    }));

    // Opt into a tight max_chars budget so the resume must truncate, but should still emit at
    // least one openable REFERENCE line for navigation.
    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": { "max_chars": 2000, "refs": true } }
    }));
    let text = extract_tool_text_str(&snapshot);

    assert!(
        text.lines()
            .any(|l| l.starts_with("WARNING: BUDGET_TRUNCATED")),
        "expected BUDGET_TRUNCATED warning when max_chars is tight"
    );

    let ref_pos = text
        .lines()
        .position(|l| l.starts_with("REFERENCE: "))
        .expect("expected a REFERENCE line under truncation");
    assert!(
        ref_pos <= 1,
        "REFERENCE line must be near the top (after the state line), got index {ref_pos}"
    );

    let ref_line = text
        .lines()
        .find(|l| l.starts_with("REFERENCE: "))
        .expect("expected at least one REFERENCE line under truncation");
    let ref_id = ref_id_from_reference_line(ref_line);

    let open_line = text
        .lines()
        .find(|l| l.starts_with("open "))
        .expect("expected an open command line under truncation");
    let open_args = parse_open_command_line(open_line);
    assert_eq!(
        open_args.get("max_chars").and_then(|v| v.as_i64()),
        Some(8000),
        "open line must carry a bounded max_chars to keep the jump experience stable"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_budget_refs", "id": ref_id } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(ref) should succeed"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": open_args }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(line) should succeed"
    );
}
