#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_anchor_is_supported_and_filters_drafts_by_default() {
    let mut server = Server::start_initialized_with_args(
        "open_anchor_is_supported_and_filters_drafts_by_default",
        &["--toolset", "daily", "--workspace", "ws_open_anchor"],
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_open_anchor" } }
    }));

    let _canon = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "card": { "id": "CARD-CANON", "type": "decision", "title": "Canon", "text": "anchor canon", "tags": ["a:core"] }
        } }
    }));

    let _draft = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "card": { "id": "CARD-DRAFT", "type": "hypothesis", "title": "Draft", "text": "anchor draft", "tags": ["a:core", "v:draft"] }
        } }
    }));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": "a:core" } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(anchor) should succeed"
    );
    let opened = opened.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        opened.get("kind").and_then(|v| v.as_str()),
        Some("anchor"),
        "open(a:*) must return kind=anchor"
    );
    let cards = opened
        .get("cards")
        .and_then(|v| v.as_array())
        .expect("result.cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-CANON")),
        "default open(anchor) should include canonical cards"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-DRAFT")),
        "default open(anchor) should exclude draft cards"
    );

    let opened_all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": "a:core", "include_drafts": true } }
    }));
    let opened_all = extract_tool_text(&opened_all);
    assert!(
        opened_all
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(anchor, include_drafts=true) should succeed"
    );
    let opened_all = opened_all.get("result").unwrap_or(&serde_json::Value::Null);
    let cards_all = opened_all
        .get("cards")
        .and_then(|v| v.as_array())
        .expect("result.cards");
    assert!(
        cards_all
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-DRAFT")),
        "include_drafts=true should include draft cards"
    );
}
