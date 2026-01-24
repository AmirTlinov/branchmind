#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn tools_call_accepts_namespace_prefixed_names_for_interop() {
    let mut server = Server::start_initialized("tool_name_namespace_compat");

    let variants = ["status", "branchmind/status", "branchmind.status"];
    for (idx, name) in variants.iter().enumerate() {
        let resp = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + idx as i64,
            "method": "tools/call",
            "params": { "name": name, "arguments": { "workspace": "ws_tool_name", "max_chars": 2000 } }
        }));
        let is_error = resp
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let text = extract_tool_text_str(&resp);
        assert!(
            !is_error,
            "status call must succeed for name variant={name}"
        );
        assert!(!text.trim().is_empty(), "tool text must be non-empty");
    }
}
