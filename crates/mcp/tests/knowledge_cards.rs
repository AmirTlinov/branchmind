#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_cards_can_be_added_and_listed() {
    let mut server = Server::start_initialized("knowledge_cards_can_be_added_and_listed");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_knowledge" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let canon = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think_add_knowledge", "arguments": { "workspace": "ws_knowledge", "anchor": "core", "card": { "title": "Invariant", "text": "Knowledge must be evidence-backed." } } }
    }));
    let canon_text = extract_tool_text(&canon);
    assert_eq!(
        canon_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let draft = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_add_knowledge", "arguments": { "workspace": "ws_knowledge", "card": { "title": "Draft", "text": "Unverified", "tags": ["v:draft"] } } }
    }));
    let draft_text = extract_tool_text(&draft);
    assert_eq!(
        draft_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "knowledge_list", "arguments": { "workspace": "ws_knowledge" } }
    }));
    let list_text = extract_tool_text(&list);
    let cards = list_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|card| card.get("type").and_then(|v| v.as_str()) == Some("knowledge")),
        "knowledge cards should be returned"
    );
    assert!(
        cards.iter().any(|card| {
            card.get("tags")
                .and_then(|v| v.as_array())
                .map(|tags| tags.iter().any(|t| t.as_str() == Some("a:core")))
                .unwrap_or(false)
        }),
        "anchor tag should be present on knowledge cards"
    );
    assert!(
        cards.iter().all(|card| {
            card.get("tags")
                .and_then(|v| v.as_array())
                .map(|tags| !tags.iter().any(|t| t.as_str() == Some("v:draft")))
                .unwrap_or(true)
        }),
        "draft knowledge should be hidden by default"
    );

    let list_all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "knowledge_list", "arguments": { "workspace": "ws_knowledge", "include_drafts": true } }
    }));
    let list_all_text = extract_tool_text(&list_all);
    let all_cards = list_all_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        all_cards.len() >= cards.len(),
        "include_drafts should not reduce results"
    );
    assert!(
        all_cards.iter().any(|card| {
            card.get("tags")
                .and_then(|v| v.as_array())
                .map(|tags| tags.iter().any(|t| t.as_str() == Some("v:draft")))
                .unwrap_or(false)
        }),
        "include_drafts should surface draft knowledge"
    );
}
