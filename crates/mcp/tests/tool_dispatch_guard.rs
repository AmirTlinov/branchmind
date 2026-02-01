#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn v1_surface_is_strict_10_tools() {
    let mut server = Server::start_initialized_with_args("v1_surface_is_strict_10_tools", &[]);

    let tools_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    assert_eq!(tools.len(), 10, "tools/list must expose exactly 10 tools");
    let mut names = std::collections::BTreeSet::new();
    for tool in tools {
        if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
            names.insert(name.to_string());
        }
    }
    let expected = [
        "status",
        "open",
        "workspace",
        "tasks",
        "jobs",
        "think",
        "graph",
        "vcs",
        "docs",
        "system",
    ];
    let expected = expected
        .into_iter()
        .map(|s| s.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, expected, "tools/list surface mismatch");
}

#[test]
fn advertised_tools_are_dispatchable() {
    let mut server = Server::start_initialized_with_args(
        "advertised_tools_are_dispatchable",
        &["--toolset", "full"],
    );

    let tools_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let mut unplugged = Vec::new();
    for (idx, tool) in tools.iter().enumerate() {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let resp = server.request(json!({
            "jsonrpc": "2.0",
            "id": 1000 + idx as i64,
            "method": "tools/call",
            "params": { "name": name, "arguments": {} }
        }));

        let is_error = resp
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_error {
            continue;
        }

        let text = extract_tool_text(&resp);
        if let Some(code) = text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
        {
            if code == "UNKNOWN_TOOL" {
                unplugged.push(name.to_string());
            }
        } else if let Some(raw) = text.as_str()
            && raw.contains("UNKNOWN_TOOL")
        {
            unplugged.push(name.to_string());
        }
    }

    assert!(
        unplugged.is_empty(),
        "tools advertised but not dispatchable: {unplugged:?}"
    );
}
