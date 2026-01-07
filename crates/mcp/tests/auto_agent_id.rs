#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn agent_id_auto_persists_across_restarts() {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let storage_dir = base.join(format!("bm_mcp_auto_agent_id_{pid}_{nonce}"));

    {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--agent-id", "auto"], false);
        server.initialize_default();

        let _ = server.request(json!( {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "init", "arguments": { "workspace": "ws_auto_agent_id" } }
        }));

        let _ = server.request(json!( {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "think_card", "arguments": {
                "workspace": "ws_auto_agent_id",
                "card": { "id": "CARD-AUTO", "type": "hypothesis", "title": "Auto", "text": "persisted lane" }
            } }
        }));
    }

    {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--agent-id", "auto"], true);
        server.initialize_default();

        let watch = server.request(json!( {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "think_watch", "arguments": { "workspace": "ws_auto_agent_id", "limit_hypotheses": 10, "limit_candidates": 10 } }
        }));
        let watch_text = extract_tool_text(&watch);
        let candidates = watch_text
            .get("result")
            .and_then(|v| v.get("candidates"))
            .and_then(|v| v.as_array())
            .expect("candidates");
        assert!(
            candidates
                .iter()
                .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-AUTO")),
            "auto agent id should persist and keep lane-visible cards across restarts"
        );
    }
}
