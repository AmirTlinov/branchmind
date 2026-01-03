#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn branchmind_tools_are_unprefixed_and_legacy_names_are_rejected() {
    let mut server = Server::start_initialized("no_legacy_names");

    let tools_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");
    for tool in tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            !name.starts_with("branchmind_"),
            "no advertised tool may use the branchmind_ prefix"
        );
    }

    let legacy = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branchmind_status", "arguments": { "workspace": "ws_legacy" } }
    }));
    let legacy_text = extract_tool_text(&legacy);
    assert_eq!(
        legacy_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_TOOL"),
        "legacy tool names must be rejected"
    );
}
