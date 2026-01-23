#![forbid(unsafe_code)]

use super::support::*;
use serde_json::json;

#[test]
fn branchmind_skill_is_budget_safe_and_line_protocol() {
    let mut server = Server::start_initialized_with_args(
        "branchmind_skill_is_budget_safe_and_line_protocol",
        &["--toolset", "daily"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "skill",
            "arguments": {
                "profile": "teamlead",
                "max_chars": 220
            }
        }
    }));

    assert_eq!(
        resp.get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "skill call must not be an MCP error"
    );

    let text = extract_tool_text_str(&resp);
    assert!(
        !text.trim_start().starts_with('{'),
        "skill must return raw text (line protocol), not JSON envelopes"
    );
    assert!(
        text.contains("skill profile=teamlead"),
        "skill output must identify the selected profile"
    );
    assert!(
        text.contains("[TEAMLEAD]") && text.contains("tasks_jobs_radar"),
        "teamlead pack should contain delegation/inbox guidance"
    );
    assert!(
        text.ends_with("..."),
        "skill output must truncate deterministically under small max_chars"
    );
}
