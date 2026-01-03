#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_note_is_mirrored_into_reasoning_notes() {
    let mut server = Server::start_initialized("tasks_note_mirrored_into_reasoning_notes");

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

    let decompose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "tasks_decompose",
            "arguments": {
                "workspace": "ws1",
                "task": task_id.clone(),
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"] }
                ]
            }
        }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let step_id = decompose_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    let note_content = "Hello from tasks_note";
    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_note", "arguments": { "workspace": "ws1", "task": task_id.clone(), "step_id": step_id, "note": note_content } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let export = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "export", "arguments": { "workspace": "ws1", "target": task_id, "notes_limit": 50, "trace_limit": 10, "max_chars": 10000 } }
    }));
    let export_text = extract_tool_text(&export);
    let export_notes_entries = export_text
        .get("result")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(
        export_notes_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(note_content)),
        "expected export to include mirrored tasks_note content"
    );
}
