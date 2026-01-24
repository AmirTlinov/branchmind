#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

fn focus_id_from_portal_line(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    assert!(
        first.starts_with("focus "),
        "expected a portal state line starting with `focus ...`, got: {first}"
    );
    first
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[test]
fn open_task_compact_is_minimal() {
    let mut server = Server::start_initialized_with_args(
        "open_task_compact_is_minimal",
        &["--toolset", "daily", "--workspace", "ws_open_compact"],
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Compact Task" } }
    }));
    let task_id = focus_id_from_portal_line(&extract_tool_text_str(&started));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": task_id, "verbosity": "compact" } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(
        opened.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "open compact should succeed"
    );
    let result = opened.get("result").unwrap();
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("task"),
        "open compact should preserve kind"
    );
    assert!(result.get("capsule").is_none(), "compact must omit capsule");
    assert!(
        result.get("focus").is_some() || result.get("next_action").is_some(),
        "compact should keep minimal navigation"
    );
}
