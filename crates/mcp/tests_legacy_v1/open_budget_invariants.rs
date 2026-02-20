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

fn warning_codes(resp: &serde_json::Value) -> Vec<String> {
    resp.get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| {
            w.get("code")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

#[test]
fn open_budget_truncation_flags_are_consistent_when_inner_resume_truncates() {
    let mut server = Server::start_initialized_with_args(
        "open_budget_truncation_flags_are_consistent_when_inner_resume_truncates",
        &["--toolset", "daily", "--workspace", "ws_open_budget_flags"],
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
            "task_title": "Open budget invariants",
            "description": "x".repeat(20_000)
        } } }
    }));
    let task_id = focus_id_from_portal_line(&extract_tool_text_str(&started));

    // max_chars is large enough that `open` itself should not need to truncate further, but
    // `open` delegates to resume_super with a smaller internal budget. If the inner resume truncates,
    // the outer open response must keep truncation explicit and consistent.
    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": task_id, "max_chars": 8000 } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(
        opened.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "open must succeed: {opened}"
    );

    let result = opened.get("result").expect("open result");
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(task_id.as_str()),
        "open must preserve id"
    );
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("task"),
        "open must preserve kind"
    );

    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected open result to be truncated: {result}");

    let budget_truncated = result
        .get("budget")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        budget_truncated,
        "expected budget.truncated to match truncation state: {result}"
    );

    let codes = warning_codes(&opened);
    assert!(
        codes.iter().any(|c| c == "BUDGET_TRUNCATED"),
        "expected BUDGET_TRUNCATED warning, got codes={codes:?}\nopened={opened}"
    );
    let uniq: std::collections::HashSet<_> = codes.iter().collect();
    assert_eq!(uniq.len(), codes.len(), "warnings must be de-duplicated");
}

#[test]
fn open_ultratight_full_preserves_stable_binding_under_budget_minimal() {
    let mut server = Server::start_initialized_with_args(
        "open_ultratight_full_preserves_stable_binding_under_budget_minimal",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_open_budget_ultratight",
        ],
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
            "task_title": "Open ultra-tight budget",
            "description": "y".repeat(20_000)
        } } }
    }));
    let task_id = focus_id_from_portal_line(&extract_tool_text_str(&started));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": task_id, "max_chars": 400 } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(
        opened.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "open must succeed: {opened}"
    );

    let result = opened.get("result").expect("open result");
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(task_id.as_str()),
        "even under minimal budgets, open must preserve id for navigation: {result}"
    );
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("task"),
        "even under minimal budgets, open must preserve kind: {result}"
    );
    assert!(
        result
            .get("workspace")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.trim().is_empty()),
        "even under minimal budgets, open must preserve workspace: {result}"
    );

    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(truncated, "expected open result to be truncated: {result}");

    let budget_truncated = result
        .get("budget")
        .and_then(|v| v.get("truncated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        budget_truncated,
        "expected budget.truncated to remain explicit under tight budgets: {result}"
    );

    let codes = warning_codes(&opened);
    let uniq: std::collections::HashSet<_> = codes.iter().collect();
    assert_eq!(uniq.len(), codes.len(), "warnings must be de-duplicated");
}
