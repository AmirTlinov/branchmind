#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn tasks_snapshot_auto_truncation_emits_openable_refs_without_budget_opt_in() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_auto_truncation_emits_openable_refs_without_budget_opt_in",
        &["--workspace", "ws_auto_trunc_refs"],
    );

    let _started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "task_title": "Auto Trunc Refs Task" } }
    }));

    // Create a very large payload so the portal's implicit/default budgets must trim.
    let huge_note = "n".repeat(300_000);
    let _note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_note",
            "arguments": {
                "workspace": "ws_auto_trunc_refs",
                "path": "s:0",
                "note": huge_note
            }
        }
    }));

    let huge_text = "d".repeat(300_000);
    let _decision = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_add_decision",
            "arguments": {
                "workspace": "ws_auto_trunc_refs",
                "card": {
                    "title": "Decision: huge content (auto trunc refs)",
                    "text": huge_text
                }
            }
        }
    }));

    // No explicit max_chars/context_budget: the portal applies its own bounded defaults.
    // When trimming happens, it must still emit REFERENCE lines for navigation.
    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": {} }
    }));
    let text = extract_tool_text_str(&snapshot);

    let state = text.lines().next().unwrap_or("");
    assert!(
        state.contains("trimmed(") || state.contains("BUDGET_TRUNCATED"),
        "expected the snapshot to be trimmed under default budgets (state line should include trimmed(..) or a budget warning)"
    );

    let ref_id =
        parse_state_ref_id(state).expect("expected ref=... handle to survive implicit truncation");
    assert!(
        ref_id.starts_with("CARD-") || ref_id.starts_with("TASK-") || ref_id.starts_with("PLAN-"),
        "expected ref= to be an openable id, got: {ref_id}"
    );
}
