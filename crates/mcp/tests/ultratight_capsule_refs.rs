#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn parse_focus_id_from_first_line(text: &str) -> String {
    let first = text.lines().next().unwrap_or("");
    assert!(
        first.starts_with("focus "),
        "expected first line to start with `focus`, got: {first}"
    );
    first
        .split_whitespace()
        .nth(1)
        .expect("expected focus id token")
        .trim()
        .to_string()
}

#[test]
fn resume_super_ultratight_preserves_capsule_refs_for_navigation() {
    let mut server = Server::start_initialized("resume_super_ultratight_preserves_capsule_refs");

    let delegated = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_delegate",
            "arguments": {
                "workspace": "ws_ultratight_refs",
                "plan_title": "Plan Ultra-tight Refs",
                "task_title": "Delegated Task (ultratight refs)"
            }
        }
    }));
    let delegated_text = extract_tool_text_str(&delegated);
    let task_id = parse_focus_id_from_first_line(&delegated_text);
    assert!(
        task_id.starts_with("TASK-"),
        "expected TASK-* focus id, got: {task_id}"
    );

    // Force truncation deterministically so the budget machinery enters the capsule-only path.
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_note",
            "arguments": {
                "workspace": "ws_ultratight_refs",
                "path": "s:0",
                "note": "x".repeat(40_000)
            }
        }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_ultratight_refs",
                "task": task_id,
                "view": "smart",
                "max_chars": 900
            }
        }
    }));
    let parsed = extract_tool_text(&resume);

    let budget_truncated = parsed
        .get("result")
        .and_then(|v| v.get("budget"))
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        budget_truncated,
        "expected resume_super budget to be truncated under max_chars=900, got:\n{parsed}"
    );

    let refs = parsed
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    assert!(
        refs.iter().any(|r| {
            r.get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.trim().starts_with("CARD-"))
        }),
        "expected capsule.refs to preserve a CARD-* jump handle even under ultra-tight budgets, got refs={refs:?}\nparsed={parsed}"
    );
}
