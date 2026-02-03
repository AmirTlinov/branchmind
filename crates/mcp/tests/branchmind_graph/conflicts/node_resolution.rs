#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn branchmind_graph_node_conflict_resolution_smoke() {
    let mut server = Server::start_initialized("branchmind_graph_node_conflict_resolution_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_node", "kind": "plan", "title": "Plan A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_node", "kind": "task", "parent": plan_id, "title": "Task A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.decompose", "args": { "workspace": "ws_graph_node", "task": task_id.clone(), "steps": [ { "title": "S1", "success_criteria": ["c1"] } ] } } }
    }));
    let decompose_text = extract_tool_text(&decompose);
    let task_id = decompose_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let node_id = "n1";
    let initial_title = "Initial title";
    let apply_initial = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_node", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": initial_title } ] } } }
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

    let derived_branch = format!("{base_branch}/graph_alt");
    let branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_graph_node", "name": derived_branch.clone(), "from": base_branch.clone() } } }
    }));
    let branch_create_text = extract_tool_text(&branch_create);
    assert_eq!(
        branch_create_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let base_title = "Base title";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_node", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": base_title } ] } } }
    }));

    let derived_title = "Derived title";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_node", "branch": derived_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": derived_title } ] } } }
    }));

    let diff = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.diff", "args": { "workspace": "ws_graph_node", "from": base_branch.clone(), "to": derived_branch.clone(), "doc": doc.clone(), "limit": 50 } } }
    }));
    let diff_text = extract_tool_text(&diff);
    assert_eq!(
        diff_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let changes = diff_text
        .get("result")
        .and_then(|v| v.get("changes"))
        .and_then(|v| v.as_array())
        .expect("diff changes");
    let node_change = changes.iter().find(|c| {
        c.get("kind").and_then(|v| v.as_str()) == Some("node")
            && c.get("id").and_then(|v| v.as_str()) == Some(node_id)
    });
    let node_change = node_change.expect("expected node change for n1");
    let change_title = node_change
        .get("to")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("to.title");
    assert_eq!(change_title, derived_title);

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_node", "from": derived_branch.clone(), "into": base_branch.clone(), "doc": doc.clone(), "limit": 200 } } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflicts_created = merge_text
        .get("result")
        .and_then(|v| v.get("conflicts_created"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(conflicts_created, 1, "expected exactly one conflict");
    let conflict_ids = merge_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert_eq!(conflict_ids.len(), 1, "expected one conflict_id");

    let conflicts_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflicts", "args": { "workspace": "ws_graph_node", "into": base_branch.clone(), "doc": doc.clone(), "limit": 50 } } }
    }));
    let conflicts_list_text = extract_tool_text(&conflicts_list);
    assert_eq!(
        conflicts_list_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflicts = conflicts_list_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert_eq!(conflicts.len(), 1, "expected exactly one conflict summary");
    let conflict_id = conflicts[0]
        .get("conflict_id")
        .and_then(|v| v.as_str())
        .expect("conflict_id")
        .to_string();

    let invalid_conflict_show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.show", "args": { "workspace": "ws_graph_node", "conflict_id": "CONFLICT-xyz" } } }
    }));
    let invalid_conflict_show_text = extract_tool_text(&invalid_conflict_show);
    assert_eq!(
        invalid_conflict_show_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    let invalid_conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.resolve", "args": { "workspace": "ws_graph_node", "conflict_id": "CONFLICT-xyz", "resolution": "use_from" } } }
    }));
    let invalid_conflict_resolve_text = extract_tool_text(&invalid_conflict_resolve);
    assert_eq!(
        invalid_conflict_resolve_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    let conflict_show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.show", "args": { "workspace": "ws_graph_node", "conflict_id": conflict_id.clone() } } }
    }));
    let conflict_show_text = extract_tool_text(&conflict_show);
    assert_eq!(
        conflict_show_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict = conflict_show_text
        .get("result")
        .and_then(|v| v.get("conflict"))
        .expect("conflict");
    assert_eq!(conflict.get("kind").and_then(|v| v.as_str()), Some("node"));
    let base_title_shown = conflict
        .get("base")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("base.title");
    assert_eq!(base_title_shown, initial_title);
    let theirs_title_shown = conflict
        .get("theirs")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("theirs.title");
    assert_eq!(theirs_title_shown, derived_title);
    let ours_title_shown = conflict
        .get("ours")
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .expect("ours.title");
    assert_eq!(ours_title_shown, base_title);

    let conflict_resolve = server.request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.resolve", "args": { "workspace": "ws_graph_node", "conflict_id": conflict_id.clone(), "resolution": "use_from" } } }
    }));
    let conflict_resolve_text = extract_tool_text(&conflict_resolve);
    assert_eq!(
        conflict_resolve_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let applied = conflict_resolve_text
        .get("result")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        applied,
        "expected applied=true when resolving with use_from"
    );

    let query_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.query", "args": { "workspace": "ws_graph_node", "branch": base_branch.clone(), "doc": doc.clone(), "ids": [node_id], "include_edges": false, "limit": 10 } } }
    }));
    let query_base_text = extract_tool_text(&query_base);
    assert_eq!(
        query_base_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_base_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    let node = nodes
        .iter()
        .find(|n| n.get("id").and_then(|v| v.as_str()) == Some(node_id))
        .expect("node n1");
    let final_title = node
        .get("title")
        .and_then(|v| v.as_str())
        .expect("node.title");
    assert_eq!(final_title, derived_title);

    let validate_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.validate", "args": { "workspace": "ws_graph_node", "branch": base_branch, "doc": doc, "max_errors": 50, "max_chars": 2000 } } }
    }));
    let validate_base_text = extract_tool_text(&validate_base);
    assert_eq!(
        validate_base_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let ok = validate_base_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(ok, "expected ok=true after conflict resolution");
}
