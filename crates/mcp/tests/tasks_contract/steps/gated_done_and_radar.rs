#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_steps_gated_done_and_radar() {
    let mut server = Server::start_initialized("tasks_steps_gated_done_and_radar");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
    }));

    let decomposed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_decompose",
            "arguments": {
                "workspace": "ws1",
                "task": task_id,
                "steps": [
                    { "title": "Step 1", "success_criteria": ["ok"] }
                ]
            }
        }
    }));
    let decomposed_text = extract_tool_text(&decomposed);
    let step_id = decomposed_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step_id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1" } }
    }));
    let radar_text = extract_tool_text(&radar);
    let focused_task_id = radar_text
        .get("result")
        .and_then(|v| v.get("target"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("radar target id")
        .to_string();
    let verify = radar_text
        .get("result")
        .and_then(|v| v.get("radar"))
        .and_then(|v| v.get("verify"))
        .and_then(|v| v.as_array())
        .expect("radar.verify");
    assert!(
        !verify.is_empty(),
        "radar.verify must reflect missing checkpoints"
    );

    let done_without_verify = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws1", "task": focused_task_id.clone(), "step_id": step_id.clone() } }
    }));
    assert_eq!(
        done_without_verify
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let done_text = extract_tool_text(&done_without_verify);
    assert_eq!(
        done_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("CHECKPOINTS_NOT_CONFIRMED")
    );
    let suggestions = done_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .expect("suggestions");
    assert_eq!(
        suggestions[0].get("target").and_then(|v| v.as_str()),
        Some("tasks_verify")
    );

    let verify_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_verify", "arguments": { "workspace": "ws1", "task": focused_task_id.clone(), "step_id": step_id.clone(), "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } } } }
    }));
    let verify_step_text = extract_tool_text(&verify_step);
    assert_eq!(
        verify_step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws1", "task": focused_task_id, "step_id": step_id } }
    }));
    let done_text2 = extract_tool_text(&done);
    assert_eq!(
        done_text2.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}
