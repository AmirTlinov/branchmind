#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn v1_golden_ops_are_plugged() {
    let mut server = Server::start_initialized_with_args(
        "v1_golden_ops_are_plugged",
        &["--toolset", "daily", "--workspace", "ws_v1_ops_plugged"],
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

    let mut unplugged = Vec::<String>::new();
    let mut idx = 0i64;

    for tool in tools {
        let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(op_enum) = tool
            .get("inputSchema")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.get("op"))
            .and_then(|v| v.get("enum"))
            .and_then(|v| v.as_array())
        else {
            continue;
        };

        for op in op_enum.iter().filter_map(|v| v.as_str()) {
            if op == "call" {
                continue;
            }
            idx += 1;
            let resp = server.request_raw(json!({
                "jsonrpc": "2.0",
                "id": 1000 + idx,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {
                        "op": op,
                        "args": {}
                    }
                }
            }));
            let text = extract_tool_text(&resp);
            let code = text
                .get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if code == "UNKNOWN_OP" {
                unplugged.push(format!("{name}.{op}"));
            }
        }
    }

    assert!(
        unplugged.is_empty(),
        "golden ops advertised in tools/list must never be unplugged: {unplugged:?}"
    );
}
