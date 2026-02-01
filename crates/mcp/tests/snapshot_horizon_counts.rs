#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn tasks_snapshot_plan_includes_horizon_counts_in_state_line() {
    let mut server = Server::start_initialized_with_args(
        "tasks_snapshot_plan_includes_horizon_counts_in_state_line",
        &["--workspace", "ws_snapshot_horizon"],
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "title": "Plan Horizon", "kind": "plan" }
        }
    }));
    let plan_id = extract_tool_text(&created_plan)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let mk_task = |server: &mut Server, id: u64, title: &str| -> String {
        let created = server.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "tasks_create",
                "arguments": { "kind": "task", "parent": plan_id.clone(), "title": title }
            }
        }));
        extract_tool_text(&created)
            .get("result")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .expect("task id")
            .to_string()
    };

    // 1 ACTIVE, 2 TODO (backlog), 1 DONE.
    let t_active = mk_task(&mut server, 2, "T active");
    let t_todo_1 = mk_task(&mut server, 3, "T todo 1");
    let t_todo_2 = mk_task(&mut server, 4, "T todo 2");
    let t_done = mk_task(&mut server, 5, "T done");

    for (id, status) in [
        (t_active, "ACTIVE"),
        (t_todo_1, "TODO"),
        (t_todo_2, "TODO"),
        (t_done, "DONE"),
    ] {
        server.request(json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "tasks_complete",
                "arguments": { "task": id, "status": status }
            }
        }));
    }

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": { "plan": plan_id, "max_chars": 1200, "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&snapshot);
    let state_line = text.lines().next().unwrap_or("");

    assert!(
        state_line.contains("horizon active=1 backlog=2 parked=0 stale=0 done=1 total=4"),
        "expected horizon counts on the state line, got:\n{state_line}\n\nfull:\n{text}"
    );
}
