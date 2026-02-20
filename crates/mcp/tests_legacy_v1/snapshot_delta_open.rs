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
fn tasks_snapshot_delta_emits_openable_refs() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_delta_emits_openable_refs",
        &["--workspace", "ws_delta_open"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Delta Open Task" } } }
    }));

    // First delta call seeds the baseline, returning an empty delta.
    let seeded = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "delta": true, "fmt": "lines" } } }
    }));
    let seeded_text = extract_tool_text_str(&seeded);
    assert!(
        !seeded_text.lines().any(|l| {
            l.starts_with("REFERENCE: NOTE ")
                || l.starts_with("REFERENCE: DECISION ")
                || l.starts_with("REFERENCE: EVIDENCE ")
                || l.starts_with("REFERENCE: CARD ")
        }),
        "baseline seed should not emit delta REFERENCE lines"
    );

    let note_text = "delta note: unique";
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.note", "args": {
                "workspace": "ws_delta_open",
                "path": "s:0",
                "note": note_text
            } } }
    }));

    let decision_title = "Decision: delta smoke";
    let decision_text = "We choose option A because it keeps the portal low-noise.";
    let _decision = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.decision", "args": {
                "workspace": "ws_delta_open",
                "card": {
                    "title": decision_title,
                    "text": decision_text
                }
            } } }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "delta": true, "fmt": "lines" } } }
    }));
    let text = extract_tool_text_str(&snapshot);

    let note_line = text
        .lines()
        .find(|l| l.starts_with("REFERENCE: NOTE "))
        .expect("expected NOTE reference line");
    let note_ref = ref_id_from_reference_line(note_line);
    assert!(note_ref.contains('@'), "NOTE ref must contain @");

    let decision_line = text
        .lines()
        .find(|l| l.starts_with("REFERENCE: DECISION "))
        .expect("expected DECISION reference line");
    let decision_id = ref_id_from_reference_line(decision_line);
    assert!(
        decision_id.starts_with("CARD-"),
        "DECISION id must be CARD-*"
    );

    let open_line = text
        .lines()
        .find(|l| l.starts_with("open "))
        .expect("expected an open command line in delta mode");
    let open_args = parse_open_command_line(open_line);
    assert_eq!(
        open_args.get("max_chars").and_then(|v| v.as_i64()),
        Some(8000),
        "open line must carry a bounded max_chars to keep the jump experience stable"
    );

    let opened_note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_delta_open", "id": note_ref } }
    }));
    let opened_note = extract_tool_text(&opened_note);
    assert!(
        opened_note
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(note) should succeed"
    );
    let opened_note = opened_note
        .get("result")
        .unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        opened_note.get("kind").and_then(|v| v.as_str()),
        Some("doc_entry")
    );
    let content = opened_note
        .get("entry")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        content.contains(note_text),
        "opened note must contain the original content"
    );

    let opened_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_delta_open", "id": decision_id } }
    }));
    let opened_card = extract_tool_text(&opened_card);
    assert!(
        opened_card
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(card) should succeed"
    );
    let opened_card = opened_card
        .get("result")
        .unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        opened_card.get("kind").and_then(|v| v.as_str()),
        Some("card")
    );
    assert_eq!(
        opened_card
            .get("card")
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str()),
        Some(decision_title)
    );
    let card_text = opened_card
        .get("card")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        card_text.contains(decision_text),
        "opened decision card must contain the original text"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
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
