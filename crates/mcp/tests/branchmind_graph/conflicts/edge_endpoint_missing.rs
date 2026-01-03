#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn branchmind_graph_edge_conflict_can_create_missing_endpoint_validation_error() {
    let mut server =
        Server::start_initialized("branchmind_graph_edge_conflict_missing_endpoint_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_edge", "kind": "plan", "title": "Plan A" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_graph_edge", "kind": "task", "parent": plan_id, "title": "Task A" } }
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
        "params": { "name": "tasks_decompose", "arguments": { "workspace": "ws_graph_edge", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let task_id = decompose_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let apply_initial = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "graph_apply", "arguments": { "workspace": "ws_graph_edge", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": "seed", "type": "idea", "title": "Seed" } ] } }
    }));
    let apply_initial_text = extract_tool_text(&apply_initial);
    assert_eq!(
        apply_initial_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let base_branch = apply_initial_text
        .get("result")
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("branch")
        .to_string();
    let doc = apply_initial_text
        .get("result")
        .and_then(|v| v.get("doc"))
        .and_then(|v| v.as_str())
        .expect("doc")
        .to_string();

    let edge_from = "edge_from";
    let edge_to = "edge_to";
    let apply_edge_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "graph_apply", "arguments": { "workspace": "ws_graph_edge", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "node_upsert", "id": edge_from, "type": "idea", "title": "Edge From" },
            { "op": "node_upsert", "id": edge_to, "type": "idea", "title": "Edge To" },
            { "op": "edge_upsert", "from": edge_from, "rel": "supports", "to": edge_to }
        ] } }
    }));
    let apply_edge_base_text = extract_tool_text(&apply_edge_base);
    assert_eq!(
        apply_edge_base_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let edge_branch = format!("{base_branch}/edge_alt");
    let edge_branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branch_create", "arguments": { "workspace": "ws_graph_edge", "name": edge_branch.clone(), "from": base_branch.clone() } }
    }));
    let edge_branch_create_text = extract_tool_text(&edge_branch_create);
    assert_eq!(
        edge_branch_create_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph_apply", "arguments": { "workspace": "ws_graph_edge", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "node_delete", "id": edge_to }
        ] } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph_apply", "arguments": { "workspace": "ws_graph_edge", "branch": edge_branch.clone(), "doc": doc.clone(), "ops": [
            { "op": "edge_upsert", "from": edge_from, "rel": "supports", "to": edge_to, "meta": { "source": "derived" } }
        ] } }
    }));

    let edge_merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "graph_merge", "arguments": { "workspace": "ws_graph_edge", "from": edge_branch.clone(), "into": base_branch.clone(), "doc": doc.clone(), "limit": 200 } }
    }));
    let edge_merge_text = extract_tool_text(&edge_merge);
    assert_eq!(
        edge_merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let edge_conflicts_created = edge_merge_text
        .get("result")
        .and_then(|v| v.get("conflicts_created"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(edge_conflicts_created, 1, "expected one edge conflict");
    let edge_conflict_ids = edge_merge_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("edge conflict_ids");
    let edge_conflict_id = edge_conflict_ids
        .first()
        .and_then(|v| v.as_str())
        .expect("edge conflict id");

    let edge_conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "graph_conflict_resolve", "arguments": { "workspace": "ws_graph_edge", "conflict_id": edge_conflict_id, "resolution": "use_from" } }
    }));
    let edge_conflict_resolve_text = extract_tool_text(&edge_conflict_resolve);
    assert_eq!(
        edge_conflict_resolve_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let validate_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "graph_validate", "arguments": { "workspace": "ws_graph_edge", "branch": base_branch, "doc": doc, "max_errors": 50, "max_chars": 2000 } }
    }));
    let validate_after_text = extract_tool_text(&validate_after);
    assert_eq!(
        validate_after_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let ok_after = validate_after_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(!ok_after, "expected ok=false for missing edge endpoint");
    let errors = validate_after_text
        .get("result")
        .and_then(|v| v.get("errors"))
        .and_then(|v| v.as_array())
        .expect("errors");
    let has_missing_endpoint = errors
        .iter()
        .any(|e| e.get("code").and_then(|v| v.as_str()) == Some("EDGE_ENDPOINT_MISSING"));
    assert!(
        has_missing_endpoint,
        "expected EDGE_ENDPOINT_MISSING in validation errors"
    );
}
