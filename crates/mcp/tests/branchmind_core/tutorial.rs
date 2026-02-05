#![forbid(unsafe_code)]

use super::support::*;
use serde_json::json;

#[test]
fn branchmind_tutorial_is_guided_and_actionable() {
    let mut server = Server::start_initialized("branchmind_tutorial_guided");

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "workspace": "ws_tutorial", "op": "call", "cmd": "system.tutorial", "args": { "limit": 2 } } }
    }));

    let out = extract_tool_text(&resp);
    assert_eq!(
        out.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tutorial must succeed"
    );

    let result = out.get("result").expect("tutorial result");
    let steps = result
        .get("steps")
        .and_then(|v| v.as_array())
        .expect("tutorial steps must be an array");
    assert_eq!(steps.len(), 2, "limit=2 must bound steps");
    assert_eq!(
        result.get("truncated").and_then(|v| v.as_bool()),
        Some(true),
        "limit should mark truncated output"
    );

    let actions = out
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("tutorial actions");
    assert_eq!(actions.len(), 2, "actions should match returned steps");
    assert_eq!(
        actions[0].get("tool").and_then(|v| v.as_str()),
        Some("status"),
        "first action should be status"
    );
    assert_eq!(
        actions[0]
            .get("args")
            .and_then(|v| v.get("workspace"))
            .and_then(|v| v.as_str()),
        Some("ws_tutorial"),
        "status action should include workspace"
    );
    assert_eq!(
        actions[1].get("tool").and_then(|v| v.as_str()),
        Some("tasks"),
        "second action should be tasks"
    );
    assert_eq!(
        actions[1]
            .get("args")
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("tasks.macro.start"),
        "tasks action should start the macro"
    );
}
