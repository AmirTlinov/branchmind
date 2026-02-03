#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn think_context_view_smart_is_cold_archive_and_explore_is_warm() {
    let mut server = Server::start_initialized("think_context_view_smart_is_cold_archive");

    let _init = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_relevance_view_ctx" } } }
    }));

    let _h = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_ctx",
            "card": { "id": "H1", "type": "hypothesis", "title": "H1", "text": "open", "status": "open" }
        } } }
    }));

    let _u = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_ctx",
            "card": { "id": "U1", "type": "update", "title": "U1", "text": "closed", "status": "closed" }
        } } }
    }));

    let smart = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": {
            "workspace": "ws_relevance_view_ctx",
            "view": "smart",
            "limit_cards": 20,
            "max_chars": 8000
        } } }
    }));
    let smart_text = extract_tool_text(&smart);
    let smart_cards = smart_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        smart_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("H1")),
        "smart view should include open frontier cards"
    );
    assert!(
        smart_cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("U1")),
        "smart view should keep archive cold (exclude closed fill cards)"
    );

    let explore = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": {
            "workspace": "ws_relevance_view_ctx",
            "view": "explore",
            "limit_cards": 20,
            "max_chars": 8000
        } } }
    }));
    let explore_text = extract_tool_text(&explore);
    let explore_cards = explore_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        explore_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("U1")),
        "explore view should include warm-archive fill cards"
    );
}

#[test]
fn think_context_view_audit_implies_all_lanes() {
    let mut server = Server::start_initialized("think_context_view_audit_implies_all_lanes");

    let _init = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_relevance_view_audit" } } }
    }));

    let _a = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_audit",
            "agent_id": "agent-a",
            "card": { "id": "A1", "type": "hypothesis", "title": "A1", "text": "lane a", "status": "open" }
        } } }
    }));

    let _b = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_audit",
            "agent_id": "agent-b",
            "card": { "id": "B1", "type": "hypothesis", "title": "B1", "text": "lane b", "status": "open" }
        } } }
    }));

    let audit = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": {
            "workspace": "ws_relevance_view_audit",
            "agent_id": "agent-a",
            "view": "audit",
            "limit_cards": 20,
            "max_chars": 8000
        } } }
    }));
    let audit_text = extract_tool_text(&audit);
    let cards = audit_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("B1")),
        "audit view should include other agent lanes (implies all_lanes=true)"
    );
}

#[test]
fn think_next_and_frontier_view_audit_implies_all_lanes() {
    let mut server =
        Server::start_initialized("think_next_and_frontier_view_audit_implies_all_lanes");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_relevance_view_nf" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_nf",
            "agent_id": "agent-a",
            "card": { "id": "A1", "type": "hypothesis", "title": "A1", "text": "lane a", "status": "open" }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_relevance_view_nf",
            "agent_id": "agent-b",
            "card": { "id": "B1", "type": "hypothesis", "title": "B1", "text": "lane b", "status": "open" }
        } } }
    }));

    let next = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.next", "args": {
            "workspace": "ws_relevance_view_nf",
            "agent_id": "agent-a",
            "view": "audit",
            "max_chars": 8000
        } } }
    }));
    let next_text = extract_tool_text(&next);
    let candidate_id = next_text
        .get("result")
        .and_then(|v| v.get("candidate"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        candidate_id, "B1",
        "audit view should allow think_next to pick a candidate from another lane"
    );

    let frontier = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.frontier", "args": {
            "workspace": "ws_relevance_view_nf",
            "agent_id": "agent-a",
            "view": "audit",
            "limit_hypotheses": 20,
            "max_chars": 8000
        } } }
    }));
    let frontier_text = extract_tool_text(&frontier);
    let hypotheses = frontier_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .expect("frontier.hypotheses");
    assert!(
        hypotheses
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("B1")),
        "audit view should allow think_frontier to include other lanes"
    );
}
