#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn search_tools_return_open_actions() {
    let mut server = Server::start_initialized("search_tools_return_open_actions");

    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": "ws_search_jump",
            "kind": "plan",
            "title": "Plan search jump"
        } } }
    }));
    let plan = extract_tool_text(&plan);
    assert_eq!(plan.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_id = plan
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": "ws_search_jump",
            "kind": "task",
            "parent": plan_id,
            "title": "Alpha search task"
        } } }
    }));
    let task = extract_tool_text(&task);
    assert_eq!(task.get("success").and_then(|v| v.as_bool()), Some(true));
    let task_id = task
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let search = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "workspace": "ws_search_jump", "op": "search", "args": {
            "text": "Alpha search",
            "limit": 5
        } } }
    }));
    let search = extract_tool_text(&search);
    assert_eq!(search.get("success").and_then(|v| v.as_bool()), Some(true));
    let hits = search
        .get("result")
        .and_then(|v| v.get("hits"))
        .and_then(|v| v.as_array())
        .expect("hits");
    assert!(
        hits.iter()
            .any(|h| h.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str())),
        "hits must include the task"
    );

    let actions = search
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("open")
                && a.get("args")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    == Some(task_id.as_str())
                && a.get("args")
                    .and_then(|v| v.get("include_content"))
                    .and_then(|v| v.as_bool())
                    == Some(true)
        }),
        "actions must include open(id=TASK-..) for the task: {actions:?}"
    );
}
