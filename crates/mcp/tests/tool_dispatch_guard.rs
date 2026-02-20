#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn tools_list_exposes_only_v3_markdown_surface() {
    let mut server = Server::start_initialized_with_args("v3_surface_tools_list", &[]);

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

    let mut names = std::collections::BTreeSet::new();
    for tool in tools {
        if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
            names.insert(name.to_string());
        }
    }

    let expected = ["think", "branch", "merge"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, expected, "tools/list surface mismatch");
}

#[test]
fn legacy_tools_fail_closed_with_unknown_tool() {
    let mut server = Server::start_initialized_with_args("v3_surface_unknown_legacy_tool", &[]);

    let legacy = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "status",
            "arguments": {}
        }
    }));
    let payload = extract_tool_text(&legacy);
    let code = payload
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(code, Some("UNKNOWN_TOOL"), "legacy tool must fail-closed");
}

#[test]
fn tools_list_schema_requires_workspace_and_markdown() {
    let mut server = Server::start_initialized_with_args("v3_surface_schema_required_fields", &[]);

    let tools_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    for tool in tools {
        let name = tool
            .get("name")
            .and_then(|v| v.as_str())
            .expect("tool.name");
        let required = tool
            .get("inputSchema")
            .and_then(|v| v.get("required"))
            .and_then(|v| v.as_array())
            .expect("inputSchema.required");
        let required_names = required
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let expected = ["workspace", "markdown"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            required_names, expected,
            "{name} inputSchema.required must match v3 contract"
        );
    }
}
