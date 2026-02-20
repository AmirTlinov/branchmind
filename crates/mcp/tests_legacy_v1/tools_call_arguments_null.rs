#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn tools_call_arguments_null_is_treated_as_empty_object() {
    // Ensure a default workspace exists so `status` can succeed with empty args.
    let mut server = Server::start_initialized_with_args(
        "tools_call_arguments_null_is_treated_as_empty_object",
        &["--workspace", "ws-null-args"],
    );

    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "status", "arguments": null }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "status should accept arguments:null as an empty object"
    );
}
