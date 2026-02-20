#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn agent_id_auto_persists_across_restarts() {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let storage_dir = base.join(format!("bm_mcp_auto_agent_id_{pid}_{nonce}"));

    let (task_id, step_id, expected_holder) = {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--agent-id", "auto"], false);
        server.initialize_default();

        let created_plan = server.request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_auto_agent_id", "kind": "plan", "title": "Plan A" } } }
        }));
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
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_auto_agent_id", "kind": "task", "parent": plan_id, "title": "Task A" } } }
        }));
        let created_task_text = extract_tool_text(&created_task);
        let task_id = created_task_text
            .get("result")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .expect("task id")
            .to_string();

        let decompose = server.request(json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.decompose", "args": { "workspace": "ws_auto_agent_id", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } } }
        }));
        let decompose_text = extract_tool_text(&decompose);
        let step_id = decompose_text
            .get("result")
            .and_then(|v| v.get("steps"))
            .and_then(|v| v.as_array())
            .and_then(|v| v.first())
            .and_then(|v| v.get("step_id"))
            .and_then(|v| v.as_str())
            .expect("step id")
            .to_string();

        // agent_id is required by the step lease tool, but should be injected automatically
        // when the server is configured with `--agent-id auto`.
        let claim = server.request(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.step.lease.claim", "args": { "workspace": "ws_auto_agent_id", "task": task_id.clone(), "step_id": step_id.clone() } } }
        }));
        let claim_text = extract_tool_text(&claim);
        let holder = claim_text
            .get("result")
            .and_then(|v| v.get("lease"))
            .and_then(|v| v.get("holder_agent_id"))
            .and_then(|v| v.as_str())
            .expect("lease holder_agent_id")
            .to_string();

        (task_id, step_id, holder)
    };

    {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--agent-id", "auto"], true);
        server.initialize_default();

        let get = server.request(json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.step.lease.get", "args": { "workspace": "ws_auto_agent_id", "task": task_id, "step_id": step_id } } }
        }));
        let get_text = extract_tool_text(&get);
        let holder = get_text
            .get("result")
            .and_then(|v| v.get("lease"))
            .and_then(|v| v.get("holder_agent_id"))
            .and_then(|v| v.as_str())
            .expect("lease holder_agent_id")
            .to_string();

        assert_eq!(
            holder, expected_holder,
            "auto agent id should persist across restarts for step leases"
        );
    }
}
