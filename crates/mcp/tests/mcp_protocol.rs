#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn mcp_auto_init_allows_tools_list_without_notifications() {
    let mut server = Server::start("auto_init_tools_list_v3");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert!(init.get("result").is_some(), "initialize must return result");

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

    let mut names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["branch", "merge", "think"]);
}

#[test]
fn parser_accepts_strict_single_bm_fence() {
    let mut server = Server::start_initialized("parser_accepts_single_bm_fence");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist limit=3\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn parser_rejects_non_fenced_markdown() {
    let mut server = Server::start_initialized("parser_rejects_non_fenced_markdown");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "commit branch=main commit=c1 message=hello body=world"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload.get("error").and_then(|v| v.get("code")).and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_text_outside_bm_fence() {
    let mut server = Server::start_initialized("parser_rejects_text_outside_bm_fence");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "preface\n```bm\nlist\n```\ntrailer"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload.get("error").and_then(|v| v.get("code")).and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_unknown_verb_for_tool() {
    let mut server = Server::start_initialized("parser_rejects_unknown_verb_for_tool");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "merge",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload.get("error").and_then(|v| v.get("code")).and_then(|v| v.as_str()),
        Some("UNKNOWN_VERB")
    );
}
