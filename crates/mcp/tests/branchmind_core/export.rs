#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_export_smoke() {
    let mut server = Server::start_initialized("branchmind_export_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let note_content = "export note";
    let notes_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": note_content } }
    }));
    let notes_commit_text = extract_tool_text(&notes_commit);
    assert_eq!(
        notes_commit_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let context_card_id = "CARD-CONTEXT-PACK-1";
    let context_card_text = "context pack smoke";
    let think_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": { "workspace": "ws1", "target": task_id.clone(), "card": { "id": context_card_id, "type": "note", "title": "Context pack", "text": context_card_text } } }
    }));
    let think_card_text = extract_tool_text(&think_card);
    assert_eq!(
        think_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let context_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "context_pack", "arguments": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50, "limit_cards": 10 } }
    }));
    let context_pack_text = extract_tool_text(&context_pack);
    assert_eq!(
        context_pack_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let context_cards = context_pack_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("context_pack cards");
    assert!(
        context_cards
            .iter()
            .any(|card| card.get("id").and_then(|v| v.as_str()) == Some(context_card_id)),
        "context_pack must include the newly added think card"
    );

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "export", "arguments": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50 } }
    }));
    let export_text = extract_tool_text(&export);
    assert_eq!(
        export_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let notes_entries = export_text
        .get("result")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(
        notes_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "export must include the note in notes.entries"
    );

    let trace_entries = export_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace.entries");
    assert!(
        trace_entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("task_created")),
        "export must include task_created in trace.entries"
    );

    let long_note = "x".repeat(2048);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } }
    }));

    let export_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "export", "arguments": { "workspace": "ws1", "target": task_id, "notes_limit": 50, "trace_limit": 50, "max_chars": 400 } }
    }));
    let export_budget_text = extract_tool_text(&export_budget);
    assert_eq!(
        export_budget_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let truncated = export_budget_text
        .get("result")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected truncated=true with small max_chars");
    let used = export_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .and_then(|v| v.get("used_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(9999);
    assert!(used <= 400, "budget.used_chars must not exceed max_chars");
}
