#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_think_wrappers_views_smoke() {
    let mut server = Server::start_initialized("branchmind_think_wrappers_views_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_think_wrap" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let hypo1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.hypothesis", "args": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "One" } } } }
    }));
    let hypo1_text = extract_tool_text(&hypo1);
    let _hypo1_id = hypo1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo1 id")
        .to_string();

    let hypo2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.hypothesis", "args": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Two" } } } }
    }));
    let hypo2_text = extract_tool_text(&hypo2);
    let _hypo2_id = hypo2_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo2 id")
        .to_string();

    let query = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.query", "args": { "workspace": "ws_think_wrap", "types": "hypothesis", "limit": 10 } } }
    }));
    let query_text = extract_tool_text(&query);
    assert_eq!(
        query_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let query_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 111,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.query", "args": { "workspace": "ws_think_wrap", "types": "hypothesis", "limit": 10, "max_chars": 200 } } }
    }));
    let query_budgeted_text = extract_tool_text(&query_budgeted);
    let budget = query_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let used = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let max = budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        used <= max,
        "budget.used_chars must be <= max_chars for think_query"
    );

    let frontier = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.frontier", "args": { "workspace": "ws_think_wrap" } } }
    }));
    let frontier_text = extract_tool_text(&frontier);
    assert_eq!(
        frontier_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let frontier_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 121,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.frontier", "args": { "workspace": "ws_think_wrap", "max_chars": 200 } } }
    }));
    let frontier_budgeted_text = extract_tool_text(&frontier_budgeted);
    let frontier_budget = frontier_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let frontier_used = frontier_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let frontier_max = frontier_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        frontier_used <= frontier_max,
        "budget.used_chars must be <= max_chars for think_frontier"
    );
    let frontier_result = frontier_budgeted_text
        .get("result")
        .expect("frontier result");
    assert!(
        frontier_result.get("frontier").is_some() || frontier_result.get("signal").is_some(),
        "think_frontier should return minimal frontier or signal under tiny max_chars"
    );

    let next = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.next", "args": { "workspace": "ws_think_wrap" } } }
    }));
    let next_text = extract_tool_text(&next);
    assert_eq!(
        next_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let next_budgeted = server.request(json!({
        "jsonrpc": "2.0",
        "id": 131,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.next", "args": { "workspace": "ws_think_wrap", "max_chars": 120 } } }
    }));
    let next_budgeted_text = extract_tool_text(&next_budgeted);
    let next_budget = next_budgeted_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let next_used = next_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.used_chars");
    let next_max = next_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("budget.max_chars");
    assert!(
        next_used <= next_max,
        "budget.used_chars must be <= max_chars for think_next"
    );
    let next_result = next_budgeted_text.get("result").expect("next result");
    assert!(
        next_result.get("candidate").is_some() || next_result.get("signal").is_some(),
        "think_next should return minimal candidate or signal under tiny max_chars"
    );

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.pack", "args": { "workspace": "ws_think_wrap", "limit_candidates": 10 } } }
    }));
    let pack_text = extract_tool_text(&pack);
    assert_eq!(
        pack_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pack_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 114,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.pack", "args": { "workspace": "ws_think_wrap", "limit_candidates": 10, "max_chars": 400 } } }
    }));
    let pack_budget_text = extract_tool_text(&pack_budget);
    let pack_budget_obj = pack_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let pack_used = pack_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let pack_max = pack_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        pack_used <= pack_max,
        "think_pack budget must not exceed max_chars"
    );
    let pack_candidates_len = pack_budget_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let pack_stats_cards = pack_budget_text
        .get("result")
        .and_then(|v| v.get("stats"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        pack_stats_cards as usize >= pack_candidates_len,
        "think_pack stats.cards must be >= candidates length"
    );

    let context_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 115,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": { "workspace": "ws_think_wrap", "limit_cards": 10, "max_chars": 400 } } }
    }));
    let context_budget_text = extract_tool_text(&context_budget);
    let context_budget_obj = context_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let context_used = context_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let context_max = context_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        context_used <= context_max,
        "think_context budget must not exceed max_chars"
    );
    let context_cards_len = context_budget_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let context_stats_cards = context_budget_text
        .get("result")
        .and_then(|v| v.get("stats"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        context_stats_cards as usize, context_cards_len,
        "think_context stats.cards must match cards length"
    );
}
