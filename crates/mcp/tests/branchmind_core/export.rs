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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } } }
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
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws1", "target": task_id.clone(), "content": note_content } } }
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
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws1", "target": task_id.clone(), "card": { "id": context_card_id, "type": "decision", "title": "Context pack", "text": context_card_text } } } }
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
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context.pack", "args": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50, "limit_cards": 10 } } }
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
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.export", "args": { "workspace": "ws1", "target": task_id.clone(), "notes_limit": 10, "trace_limit": 50 } } }
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
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } } }
    }));

    let export_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.export", "args": { "workspace": "ws1", "target": task_id, "notes_limit": 50, "trace_limit": 50, "max_chars": 400 } } }
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

#[test]
fn branchmind_context_pack_export_writes_file() {
    let mut server = Server::start_initialized("branchmind_context_pack_export_writes_file");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let note_content = "export-to-file note";
    let notes_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws1", "target": task_id.clone(), "content": note_content } } }
    }));
    let notes_commit_text = extract_tool_text(&notes_commit);
    assert_eq!(
        notes_commit_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_dir = std::env::temp_dir().join(format!("bm_context_pack_export_{pid}_{nonce}"));
    std::fs::create_dir_all(&out_dir).expect("create out_dir");
    let out_file = out_dir.join("context_pack.json");

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context.pack.export", "args": {
                "workspace": "ws1",
                "target": task_id,
                "notes_limit": 10,
                "trace_limit": 50,
                "limit_cards": 5,
                "out_file": out_file.to_string_lossy()
            } } }
    }));
    let export_text = extract_tool_text(&export);
    assert_eq!(
        export_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let raw = std::fs::read_to_string(&out_file).expect("read exported file");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse exported json");
    let notes_entries = parsed
        .get("notes")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(
        notes_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "exported context_pack must include the committed note"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}
