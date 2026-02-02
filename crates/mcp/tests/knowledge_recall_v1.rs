#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_recall_is_empty_before_any_keys_exist() {
    let mut server = Server::start_initialized("knowledge_recall_is_empty_before_any_keys_exist");

    let recall = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_empty",
                "op": "knowledge.recall",
                "args": { "limit": 12 }
            }
        }
    }));
    let text = extract_tool_text(&recall);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));

    let result = text.get("result").expect("result");
    assert_eq!(
        result.get("branch").and_then(|v| v.as_str()),
        Some("kb/main"),
        "recall should target the KB branch by default"
    );
    let cards = result
        .get("cards")
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(cards.is_empty(), "expected empty recall in fresh workspace");
}

#[test]
fn knowledge_upsert_key_is_stable_and_recall_orders_by_recency() {
    let mut server =
        Server::start_initialized("knowledge_upsert_key_is_stable_and_recall_orders_by_recency");

    let upsert_1 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "Must be deterministic." }
                }
            }
        }
    }));
    let upsert_1_text = extract_tool_text(&upsert_1);
    assert_eq!(
        upsert_1_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let card_1_id = upsert_1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("card_id")
        .to_string();

    // Ensure updated_at ordering is reliably observable at millisecond resolution.
    std::thread::sleep(std::time::Duration::from_millis(10));

    let upsert_2 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "concurrency",
                    "card": { "title": "Concurrency", "text": "Be explicit about locks." }
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

    let recall_1 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb",
                "op": "knowledge.recall",
                "args": { "anchor": "core", "limit": 10, "include_drafts": true }
            }
        }
    }));
    let recall_1_text = extract_tool_text(&recall_1);
    assert_eq!(
        recall_1_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards_1 = recall_1_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards_1.len() >= 2,
        "expected at least two recalled cards, got {}",
        cards_1.len()
    );
    assert_eq!(
        cards_1
            .first()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(card_2_id.as_str()),
        "newer card should come first"
    );

    // Update key=determinism; it must keep the same stable card_id and become the freshest.
    std::thread::sleep(std::time::Duration::from_millis(10));

    let upsert_1b = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "Still must be deterministic (updated)." }
                }
            }
        }
    }));
    let upsert_1b_text = extract_tool_text(&upsert_1b);
    assert_eq!(
        upsert_1b_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let card_1b_id = upsert_1b_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("card_id")
        .to_string();
    assert_ne!(
        card_1b_id, card_1_id,
        "updated knowledge must create a new versioned card_id"
    );

    let recall_2 = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb",
                "op": "knowledge.recall",
                "args": { "anchor": "core", "limit": 10, "include_drafts": true }
            }
        }
    }));
    let recall_2_text = extract_tool_text(&recall_2);
    assert_eq!(
        recall_2_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards_2 = recall_2_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards_2.len() >= 2,
        "expected at least two recalled cards, got {}",
        cards_2.len()
    );
    assert_eq!(
        cards_2
            .first()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some(card_1b_id.as_str()),
        "updated card should become first"
    );
}
