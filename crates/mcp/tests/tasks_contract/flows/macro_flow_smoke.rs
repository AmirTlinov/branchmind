#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_macro_flow_smoke() {
    let mut server = Server::start_initialized("tasks_macro_flow_smoke");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Macro",
                "task_title": "Task Macro",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start must succeed"
    );
    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws1" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus task id")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": {
                "workspace": "ws1",
                "checkpoints": "gate",
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&close).starts_with("ERROR:"),
        "macro_close_step must succeed"
    );

    let finish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_finish",
            "arguments": {
                "workspace": "ws1",
                "task": task_id
            }
        }
    }));
    let finish_text = extract_tool_text(&finish);
    assert!(
        finish_text
            .get("result")
            .and_then(|v| v.get("handoff"))
            .is_some()
    );
}

#[test]
fn tasks_macro_finish_is_idempotent_when_already_done() {
    let mut server = Server::start_initialized("tasks_macro_finish_idempotent");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_finish_idem",
                "plan_title": "Plan Finish",
                "task_title": "Task Finish",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "workspace": "ws_finish_idem", "checkpoints": "gate", "resume_max_chars": 4000 }
        }
    }));
    assert!(
        !extract_tool_text_str(&close).starts_with("ERROR:"),
        "macro_close_step must succeed"
    );

    let first = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_macro_finish", "arguments": { "workspace": "ws_finish_idem" } }
    }));
    let first_text = extract_tool_text(&first);
    let first_revision = first_text
        .get("result")
        .and_then(|v| v.get("handoff"))
        .and_then(|v| v.get("target"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("handoff.target.revision");

    let second = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_macro_finish", "arguments": { "workspace": "ws_finish_idem" } }
    }));
    let second_text = extract_tool_text(&second);
    let second_revision = second_text
        .get("result")
        .and_then(|v| v.get("handoff"))
        .and_then(|v| v.get("target"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("handoff.target.revision (second)");

    assert_eq!(
        second_revision, first_revision,
        "macro_finish should not emit another completion event when already DONE"
    );

    let warning_codes = second_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| w.get("code").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        warning_codes.iter().any(|c| *c == "ALREADY_DONE"),
        "second macro_finish should warn ALREADY_DONE"
    );
}

#[test]
fn tasks_macro_finish_suggests_closing_steps_when_open() {
    let mut server = Server::start_initialized("tasks_macro_finish_suggests_close_step");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_finish_open",
                "plan_title": "Plan Finish Open",
                "task_title": "Task Finish Open",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let finish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_macro_finish", "arguments": { "workspace": "ws_finish_open" } }
    }));
    let finish_text = extract_tool_text(&finish);

    assert!(
        !finish_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "macro_finish must fail when steps are still open"
    );

    let suggested_tools = finish_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("target").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    assert!(
        suggested_tools
            .iter()
            .any(|t| *t == "tasks_macro_close_step"),
        "macro_finish should suggest tasks_macro_close_step recovery"
    );
}

#[test]
fn tasks_macro_finish_appends_final_note_to_reasoning_notes() {
    let mut server = Server::start_initialized("tasks_macro_finish_appends_final_note");

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_finish_note",
                "plan_title": "Plan Finish Note",
                "task_title": "Task Finish Note",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ],
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&start).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws_finish_note" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus task id")
        .to_string();

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "workspace": "ws_finish_note", "checkpoints": "gate", "resume_max_chars": 4000 }
        }
    }));
    assert!(
        !extract_tool_text_str(&close).starts_with("ERROR:"),
        "macro_close_step must succeed"
    );

    let final_note = "Final note: shipped, risks: none, next: monitor".to_string();
    let finish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_finish",
            "arguments": {
                "workspace": "ws_finish_note",
                "task": task_id.clone(),
                "final_note": final_note
            }
        }
    }));
    let finish_text = extract_tool_text(&finish);
    assert!(
        finish_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "macro_finish must succeed"
    );

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_finish_note",
                "task": task_id,
                "view": "full",
                "notes_limit": 10,
                "max_chars": 12000
            }
        }
    }));
    let resume_text = extract_tool_text(&resume);
    let notes = resume_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let has_note = notes.iter().any(|entry| {
        entry
            .get("content")
            .and_then(|v| v.as_str())
            .is_some_and(|content| content.contains("Final note: shipped"))
    });
    assert!(has_note, "final_note must be appended into reasoning notes");
}
