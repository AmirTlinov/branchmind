#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_trace_smoke() {
    let mut server = Server::start_initialized("branchmind_trace_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_trace" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_trace", "kind": "plan", "title": "Trace Plan" } }
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
        "id": 21,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_trace", "kind": "task", "parent": plan_id, "title": "Trace Task" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_trace", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let target_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let target_trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();

    let target_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "trace_step", "arguments": { "workspace": "ws_trace", "target": task_id, "step": "Target step" } }
    }));
    let target_step_text = extract_tool_text(&target_step);
    assert_eq!(
        target_step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(target_branch.as_str())
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some(target_trace_doc.as_str())
    );

    let step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "trace_step", "arguments": { "workspace": "ws_trace", "step": "Step 1", "message": "m1" } }
    }));
    let step_text = extract_tool_text(&step);
    let seq1 = step_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace step seq");

    let seq_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "trace_sequential_step",
            "arguments": {
                "workspace": "ws_trace",
                "thought": "Thought 1",
                "thoughtNumber": 1,
                "totalThoughts": 2,
                "nextThoughtNeeded": true
            }
        }
    }));
    let seq_text = extract_tool_text(&seq_step);
    let seq2 = seq_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace sequential seq");
    assert!(seq2 > seq1, "sequential step must advance seq");

    let hydrate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "trace_hydrate", "arguments": { "workspace": "ws_trace", "limit_steps": 10 } }
    }));
    let hydrate_text = extract_tool_text(&hydrate);
    let entries = hydrate_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace entries");
    assert!(entries.len() >= 2, "trace hydrate must return entries");

    let validate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "trace_validate", "arguments": { "workspace": "ws_trace" } }
    }));
    let validate_text = extract_tool_text(&validate);
    let ok = validate_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .expect("trace validate ok");
    assert!(ok, "trace validate must be ok");
}
