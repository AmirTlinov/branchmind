#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn default_budgets_apply_to_branchmind_show_when_omitted() {
    let mut server = Server::start_initialized("default_budgets_apply_to_branchmind_show");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_auto_budget_show" } }
    }));

    let show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws_auto_budget_show", "doc_kind": "notes" } }
    }));
    let show_text = extract_tool_text(&show);
    let budget = show_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("result.budget should be present when default budgets are injected");
    let max = budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");

    assert_eq!(
        max, 16_000,
        "default show max_chars should be injected as a bounded, generous payload cap"
    );
}

#[test]
fn auto_budget_escalates_multiple_times_when_default_budget_truncates() {
    let mut server =
        Server::start_initialized("auto_budget_escalates_multiple_times_when_truncated");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_auto_budget_escalate" } }
    }));

    let huge = "x".repeat(300_000);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws_auto_budget_escalate", "content": huge } }
    }));

    // Call without max_chars/context_budget. Server injects default max_chars, sees budget truncation,
    // and retries a bounded number of times until truncation disappears (still capped).
    let show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws_auto_budget_escalate", "doc_kind": "notes" } }
    }));
    let show_text = extract_tool_text(&show);
    let budget = show_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("result.budget");
    let max = budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");

    let warnings = show_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let has_budget_warning = warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    });
    assert!(
        !has_budget_warning,
        "auto-escalation should remove BUDGET_* warnings for large-but-reasonable payloads (max_chars={max}), got: {warnings:?}"
    );
}
