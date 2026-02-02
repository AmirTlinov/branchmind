#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_query_defaults_to_latest_only_and_supports_include_history() {
    let mut server = Server::start_initialized(
        "knowledge_query_defaults_to_latest_only_and_supports_include_history",
    );

    let upsert_1 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "v1" }
                }
            }
        }
    }));
    let upsert_1_text = extract_tool_text(&upsert_1);
    assert_eq!(
        upsert_1_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let _card_1_id = upsert_1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("card_id")
        .to_string();

    std::thread::sleep(std::time::Duration::from_millis(10));

    let upsert_2 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "v2" }
                }
            }
        }
    }));
    let upsert_2_text = extract_tool_text(&upsert_2);
    assert_eq!(
        upsert_2_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let card_2_id = upsert_2_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("card_id")
        .to_string();

    // Default: latest-only (no historical duplicates).
    let query_latest = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq",
                "op": "knowledge.query",
                "args": { "anchor": "core", "key": "determinism", "limit": 20, "include_drafts": true }
            }
        }
    }));
    let query_latest_text = extract_tool_text(&query_latest);
    assert_eq!(
        query_latest_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards_latest = query_latest_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("result.cards");
    assert_eq!(
        cards_latest.len(),
        1,
        "knowledge.query should be latest-only by default (include_history=false)"
    );
    assert_eq!(
        cards_latest[0].get("id").and_then(|v| v.as_str()),
        Some(card_2_id.as_str()),
        "latest version must be returned"
    );
    assert_eq!(
        cards_latest[0].get("text").and_then(|v| v.as_str()),
        Some("v2")
    );

    // include_history=true: return historical versions.
    let query_history = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq",
                "op": "knowledge.query",
                "args": { "anchor": "core", "key": "determinism", "limit": 20, "include_history": true, "include_drafts": true }
            }
        }
    }));
    let query_history_text = extract_tool_text(&query_history);
    assert_eq!(
        query_history_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards_history = query_history_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("result.cards");
    assert_eq!(
        cards_history.len(),
        2,
        "include_history=true must return versions"
    );
    assert!(
        cards_history
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some(card_2_id.as_str())),
        "history must include latest card id"
    );
}

#[test]
fn knowledge_query_key_across_anchors_returns_latest_per_anchor() {
    let mut server =
        Server::start_initialized("knowledge_query_key_across_anchors_returns_latest_per_anchor");

    // Same key under two anchors.
    server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq_multi",
                "op": "knowledge.upsert",
                "args": { "anchor": "core", "key": "determinism", "card": { "title": "Core", "text": "core" } }
            }
        }
    }));
    server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq_multi",
                "op": "knowledge.upsert",
                "args": { "anchor": "storage", "key": "determinism", "card": { "title": "Storage", "text": "storage" } }
            }
        }
    }));

    let query = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kq_multi",
                "op": "knowledge.query",
                "args": { "key": "determinism", "limit": 20, "include_drafts": true }
            }
        }
    }));
    let text = extract_tool_text(&query);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));
    let cards = text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("result.cards");
    assert_eq!(
        cards.len(),
        2,
        "expected one latest card per anchor for a key"
    );

    let mut anchors = cards
        .iter()
        .filter_map(|c| {
            c.get("tags")
                .and_then(|v| v.as_array())
                .and_then(|tags| {
                    tags.iter()
                        .filter_map(|t| t.as_str())
                        .find(|t| t.starts_with("a:"))
                })
                .map(|t| t.to_string())
        })
        .collect::<Vec<_>>();
    anchors.sort();
    anchors.dedup();
    assert_eq!(
        anchors,
        vec!["a:core".to_string(), "a:storage".to_string()],
        "expected both anchors to be represented exactly once"
    );
}
