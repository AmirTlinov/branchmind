#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_card_include_content_returns_title_and_text() {
    let mut server = Server::start_initialized("open_card_include_content_returns_title_and_text");

    let upsert = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_open_card_content",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "open-card-content",
                    "card": { "title": "Card title", "text": "Card text" }
                }
            }
        }
    }));
    let upsert = extract_tool_text(&upsert);
    assert_eq!(upsert.get("success").and_then(|v| v.as_bool()), Some(true));
    let card_id = upsert
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("card_id")
        .to_string();

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "open",
            "arguments": {
                "workspace": "ws_open_card_content",
                "id": card_id,
                "include_content": true,
                "verbosity": "compact"
            }
        }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(opened.get("success").and_then(|v| v.as_bool()), Some(true));
    let result = opened.get("result").expect("result");
    assert_eq!(result.get("kind").and_then(|v| v.as_str()), Some("card"));

    let content = result.get("content").expect("content");
    assert_eq!(
        content.get("title").and_then(|v| v.as_str()),
        Some("Card title")
    );
    assert_eq!(
        content.get("text").and_then(|v| v.as_str()),
        Some("Card text")
    );
}
