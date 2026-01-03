#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_create_context_delta_smoke() {
    let mut server = Server::start_initialized("tasks_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
    }));
    assert_eq!(
        created_plan
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id.clone(), "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    assert_eq!(
        created_task_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let edited_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_edit", "arguments": { "workspace": "ws1", "task": plan_id, "expected_revision": 0, "title": "Plan B" } }
    }));
    let edited_text = extract_tool_text(&edited_plan);
    assert_eq!(
        edited_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        edited_text
            .get("result")
            .and_then(|v| v.get("revision"))
            .and_then(|v| v.as_i64()),
        Some(1)
    );

    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws1" } }
    }));
    let ctx_text = extract_tool_text(&context);
    let plans = ctx_text
        .get("result")
        .and_then(|v| v.get("plans"))
        .and_then(|v| v.as_array())
        .expect("plans");
    let tasks = ctx_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
        .expect("tasks");
    assert_eq!(plans.len(), 1);
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        plans[0].get("title").and_then(|v| v.as_str()),
        Some("Plan B")
    );
    assert_eq!(plans[0].get("revision").and_then(|v| v.as_i64()), Some(1));

    let context_pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 56,
        "method": "tools/call",
        "params": { "name": "tasks_context_pack", "arguments": { "workspace": "ws1", "task": task_id.clone(), "delta_limit": 50, "max_chars": 400 } }
    }));
    let context_pack_text = extract_tool_text(&context_pack);
    let pack_budget = context_pack_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let pack_used = pack_budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let pack_max = pack_budget
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        pack_used <= pack_max,
        "tasks_context_pack budget must not exceed max_chars"
    );

    let context_limited = server.request(json!({
        "jsonrpc": "2.0",
        "id": 55,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws1", "max_chars": 10 } }
    }));
    let ctx_limited_text = extract_tool_text(&context_limited);
    let limited_budget = ctx_limited_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    assert_eq!(
        limited_budget.get("truncated").and_then(|v| v.as_bool()),
        Some(true)
    );
    let limited_result = ctx_limited_text.get("result").expect("result");
    if let Some(plans) = limited_result.get("plans").and_then(|v| v.as_array()) {
        assert!(plans.is_empty());
    }
    if let Some(tasks) = limited_result.get("tasks").and_then(|v| v.as_array()) {
        assert!(tasks.is_empty());
    }

    let delta = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1" } }
    }));
    let delta_text = extract_tool_text(&delta);
    let events = delta_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .expect("events");
    assert_eq!(events.len(), 3);

    let delta_limited = server.request(json!({
        "jsonrpc": "2.0",
        "id": 66,
        "method": "tools/call",
        "params": { "name": "tasks_delta", "arguments": { "workspace": "ws1", "max_chars": 10 } }
    }));
    let delta_limited_text = extract_tool_text(&delta_limited);
    let limited_budget = delta_limited_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    assert_eq!(
        limited_budget.get("truncated").and_then(|v| v.as_bool()),
        Some(true)
    );
    if let Some(events) = delta_limited_text
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
    {
        assert!(events.is_empty());
    }
}
