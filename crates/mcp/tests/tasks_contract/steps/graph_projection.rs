#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_graph_projection_smoke() {
    let mut server = Server::start_initialized("tasks_graph_projection_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_proj", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_proj", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
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

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let graph_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("graph_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.graph_doc")
        .to_string();

    let task_node = format!("task:{task_id}");
    let step_node = format!("step:{step_id}");
    let query = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "graph_query", "arguments": { "workspace": "ws_graph_proj", "branch": branch.clone(), "doc": graph_doc.clone(), "ids": [task_node.clone(), step_node.clone()], "include_edges": true, "edges_limit": 10, "limit": 10 } }
    }));
    let query_text = extract_tool_text(&query);
    assert_eq!(
        query_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let task_node_entry = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(task_node.as_str()))
        .expect("task node");
    assert_eq!(
        task_node_entry.get("type").and_then(|v| v.as_str()),
        Some("task")
    );
    let step_node_entry = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(step_node.as_str()))
        .expect("step node");
    assert_eq!(
        step_node_entry.get("type").and_then(|v| v.as_str()),
        Some("step")
    );

    let edges = query_text
        .get("result")
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.as_array())
        .expect("edges");
    let edge = edges.iter().find(|e| {
        e.get("from").and_then(|v| v.as_str()) == Some(task_node.as_str())
            && e.get("rel").and_then(|v| v.as_str()) == Some("contains")
            && e.get("to").and_then(|v| v.as_str()) == Some(step_node.as_str())
    });
    assert!(edge.is_some(), "expected contains edge task -> step");

    let verify = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_verify", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "step_id": step_id.clone(), "checkpoints": { "criteria": { "confirmed": true }, "tests": { "confirmed": true } } } }
    }));
    let verify_text = extract_tool_text(&verify);
    assert_eq!(
        verify_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_done", "arguments": { "workspace": "ws_graph_proj", "task": task_id.clone(), "step_id": step_id.clone() } }
    }));
    let done_text = extract_tool_text(&done);
    assert_eq!(
        done_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let query_done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph_query", "arguments": { "workspace": "ws_graph_proj", "branch": branch.clone(), "doc": graph_doc.clone(), "ids": [step_node.clone()], "include_edges": false, "limit": 10 } }
    }));
    let query_done_text = extract_tool_text(&query_done);
    let done_nodes = query_done_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let done_node = done_nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(step_node.as_str()))
        .expect("step node");
    assert_eq!(
        done_node.get("status").and_then(|v| v.as_str()),
        Some("done")
    );
}
