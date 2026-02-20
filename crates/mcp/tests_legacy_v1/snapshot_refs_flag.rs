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
fn tasks_snapshot_refs_flag_emits_openable_refs() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_refs_flag_emits_openable_refs",
        &["--workspace", "ws_refs_flag"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Refs Flag Task" } } }
    }));

    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.note", "args": {
                "workspace": "ws_refs_flag",
                "path": "s:0",
                "note": "note for refs flag test"
            } } }
    }));

    // `refs=true` must emit at least one openable reference even without explicit budgets.
    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "refs": true, "fmt": "lines" } } }
    }));
    let text = extract_tool_text_str(&snapshot);

    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("WARNING: BUDGET_TRUNCATED")),
        "refs=true should not require budget truncation"
    );

    let ref_pos = text
        .lines()
        .position(|l| l.starts_with("REFERENCE: "))
        .expect("expected a REFERENCE line when refs=true");
    assert!(
        ref_pos <= 1,
        "REFERENCE line must be near the top (after the state line), got index {ref_pos}"
    );

    let ref_line = text
        .lines()
        .find(|l| l.starts_with("REFERENCE: "))
        .expect("expected at least one REFERENCE line when refs=true");
    let ref_id = ref_id_from_reference_line(ref_line);

    let open_line = text
        .lines()
        .find(|l| l.starts_with("open "))
        .expect("expected an open command line when refs=true");
    let open_args = parse_open_command_line(open_line);
    assert_eq!(
        open_args.get("max_chars").and_then(|v| v.as_i64()),
        Some(8000),
        "open line must carry a bounded max_chars to keep the jump experience stable"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_refs_flag", "id": ref_id } }
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
        "id": 5,
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
