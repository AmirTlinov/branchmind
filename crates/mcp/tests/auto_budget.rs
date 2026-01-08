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
fn auto_budget_escalates_once_when_default_budget_truncates() {
    let mut server = Server::start_initialized("auto_budget_escalates_once_when_truncated");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_auto_budget_escalate" } }
    }));

    let huge = "x".repeat(50_000);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws_auto_budget_escalate", "content": huge } }
    }));

    // Call without max_chars/context_budget. Server injects default max_chars, sees budget truncation,
    // and retries once with a larger budget (still bounded).
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

    assert_eq!(
        max, 32_000,
        "auto-escalation should retry once (16k -> 32k) when default budgets truncate"
    );

    let warnings = show_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.get("code").and_then(|v| v.as_str()) == Some("BUDGET_TRUNCATED")),
        "huge note content should still trigger BUDGET_TRUNCATED even after a single auto-escalation pass"
    );
}
