#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_notes_and_trace_ingestion_smoke() {
    let mut server = Server::start_initialized("branchmind_memory_smoke");

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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws1", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    assert_eq!(
        decompose_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let task_id = decompose_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let show_trace = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "trace", "limit": 50 } }
    }));
    let trace_text = extract_tool_text(&show_trace);
    let entries = trace_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("task_created")),
        "trace must contain task_created"
    );
    assert!(
        entries
            .iter()
            .any(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("steps_added")),
        "trace must contain steps_added"
    );

    let secret_note = "Authorization: Bearer sk-THISISSECRET0123456789012345 token=supersecret";
    let long_note = format!("{secret_note} {}", "x".repeat(2048));
    let notes_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": long_note } }
    }));
    let notes_commit_text = extract_tool_text(&notes_commit);
    assert_eq!(
        notes_commit_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let show_notes = server.request(json!({
        "jsonrpc": "2.0",
        "id": 70,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "notes", "limit": 50 } }
    }));
    let show_notes_text = extract_tool_text(&show_notes);
    let note_entries = show_notes_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    let note_content = note_entries
        .iter()
        .find(|e| e.get("kind").and_then(|v| v.as_str()) == Some("note"))
        .and_then(|e| e.get("content"))
        .and_then(|v| v.as_str())
        .expect("note content");
    assert!(!note_content.contains("sk-THISISSECRET"));
    assert!(note_content.contains("<redacted>"));

    let show_notes_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws1", "target": task_id.clone(), "doc_kind": "notes", "limit": 50, "max_chars": 400 } }
    }));
    let notes_text = extract_tool_text(&show_notes_budget);
    assert_eq!(
        notes_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let truncated = notes_text
        .get("result")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected truncated=true with small max_chars");
    let used = notes_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .and_then(|v| v.get("used_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(9999);
    assert!(used <= 400, "budget.used_chars must not exceed max_chars");
}
