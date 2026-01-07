#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_step_lease_blocks_mutations_and_surfaces_in_hud() {
    let mut server = Server::start_initialized("tasks_step_lease_blocks_mutations");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_lease", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_lease", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
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

    let claim = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_step_lease_claim", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "step_id": step_id.clone(), "agent_id": "agent_a" } }
    }));
    let claim_text = extract_tool_text(&claim);
    assert_eq!(
        claim_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let blocked = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_close_step", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": "all", "agent_id": "agent_b" } }
    }));
    let blocked_text = extract_tool_text(&blocked);
    assert_eq!(
        blocked_text.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        blocked_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("STEP_LEASE_HELD")
    );

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_resume_super", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "view": "focus_only", "agent_id": "agent_b", "max_chars": 20000 } }
    }));
    let resume_text = extract_tool_text(&resume);
    assert_eq!(
        resume_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let lease_holder = resume_text
        .get("result")
        .and_then(|v| v.get("step_focus"))
        .and_then(|v| v.get("detail"))
        .and_then(|v| v.get("lease"))
        .and_then(|v| v.get("holder_agent_id"))
        .and_then(|v| v.as_str());
    assert_eq!(lease_holder, Some("agent_a"));

    let capsule_holder = resume_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("step_focus"))
        .and_then(|v| v.get("lease"))
        .and_then(|v| v.get("holder_agent_id"))
        .and_then(|v| v.as_str());
    assert_eq!(capsule_holder, Some("agent_a"));

    let capsule_action = resume_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("tool"))
        .and_then(|v| v.as_str());
    assert_eq!(
        capsule_action,
        Some("tasks_step_lease_get"),
        "when a step is leased by another agent, the capsule should avoid suggesting a guaranteed-failing mutation"
    );

    let release = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_step_lease_release", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "step_id": step_id.clone(), "agent_id": "agent_a" } }
    }));
    let release_text = extract_tool_text(&release);
    assert_eq!(
        release_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks_close_step", "arguments": { "workspace": "ws_lease", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": "all", "agent_id": "agent_b" } }
    }));
    let close_text = extract_tool_text(&close);
    assert_eq!(
        close_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}
