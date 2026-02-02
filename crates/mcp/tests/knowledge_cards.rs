#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

fn init_workspace(server: &mut Server, workspace: &str) {
    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": workspace } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "init should succeed"
    );
}

fn list_knowledge(server: &mut Server, args: serde_json::Value) -> Vec<serde_json::Value> {
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/call",
        "params": { "name": "knowledge_list", "arguments": args }
    }));
    let text = extract_tool_text(&resp);
    text.get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .cloned()
        .expect("result.cards array")
}

#[test]
fn think_add_knowledge_defaults_to_draft() {
    let mut server = Server::start_initialized("think_add_knowledge_defaults_to_draft");
    init_workspace(&mut server, "ws_knowledge_default_draft");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think_add_knowledge",
            "arguments": {
                "workspace": "ws_knowledge_default_draft",
                "anchor": "core",
                "card": { "title": "Invariant", "text": "Knowledge must be evidence-backed." }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    assert_eq!(
        created_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think_add_knowledge should succeed"
    );

    let cards = list_knowledge(
        &mut server,
        json!({ "workspace": "ws_knowledge_default_draft" }),
    );
    let invariant = cards
        .iter()
        .find(|card| card.get("title").and_then(|v| v.as_str()) == Some("Invariant"))
        .expect("expected knowledge card with title=Invariant");
    let tags = invariant
        .get("tags")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        tags.iter().any(|t| t.as_str() == Some("v:draft")),
        "knowledge should default to v:draft (canon must be explicit)"
    );
}

#[test]
fn knowledge_list_includes_drafts_by_default() {
    let mut server = Server::start_initialized("knowledge_list_includes_drafts_by_default");
    init_workspace(&mut server, "ws_knowledge_list_defaults");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think_add_knowledge",
            "arguments": {
                "workspace": "ws_knowledge_list_defaults",
                "card": { "title": "Draft", "text": "Unverified", "tags": ["v:draft"] }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    assert_eq!(
        created_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think_add_knowledge should succeed"
    );

    let cards = list_knowledge(
        &mut server,
        json!({ "workspace": "ws_knowledge_list_defaults" }),
    );
    assert!(
        cards.iter().any(|card| {
            card.get("title").and_then(|v| v.as_str()) == Some("Draft")
                && card
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|tags| tags.iter().any(|t| t.as_str() == Some("v:draft")))
                    .unwrap_or(false)
        }),
        "draft knowledge should be included by default"
    );
}

#[test]
fn knowledge_list_defaults_to_latest_only_unless_include_history() {
    let mut server = Server::start_initialized("knowledge_list_latest_only");
    init_workspace(&mut server, "ws_knowledge_history");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think_add_knowledge",
            "arguments": {
                "workspace": "ws_knowledge_history",
                "anchor": "core",
                "key": "determinism",
                "card": { "title": "Invariant", "text": "v1" }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_add_knowledge",
            "arguments": {
                "workspace": "ws_knowledge_history",
                "anchor": "core",
                "key": "determinism",
                "card": { "title": "Invariant", "text": "v2" }
            }
        }
    }));

    let cards = list_knowledge(
        &mut server,
        json!({ "workspace": "ws_knowledge_history", "key": "determinism" }),
    );
    assert_eq!(
        cards.len(),
        1,
        "knowledge_list should return latest-only by default (include_history=false)"
    );
    assert_eq!(
        cards[0].get("text").and_then(|v| v.as_str()),
        Some("v2"),
        "latest knowledge version should be returned"
    );

    let history = list_knowledge(
        &mut server,
        json!({ "workspace": "ws_knowledge_history", "key": "determinism", "include_history": true }),
    );
    assert_eq!(
        history.len(),
        2,
        "include_history=true should return historical versions"
    );
}
