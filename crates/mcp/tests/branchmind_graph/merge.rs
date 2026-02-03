#![forbid(unsafe_code)]

use super::support::*;
use serde_json::json;

#[test]
fn branchmind_graph_merge_dry_run_preview_and_merge_to_base() {
    let mut server = Server::start_initialized("branchmind_graph_merge_preview");

    let created_plan = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_preview", "kind": "plan", "title": "Plan A" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_preview", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let node_id = "n1";
    let apply_initial = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_preview", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Initial title" } ] } } }
    }));
    let apply_initial_text = extract_tool_text(&apply_initial);
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

    let derived_branch = format!("{base_branch}/graph_preview");
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_graph_preview", "name": derived_branch.clone(), "from": base_branch.clone() } } }
    }));

    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_preview", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Base title" } ] } } }
    }));
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_preview", "branch": derived_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Derived title" } ] } } }
    }));

    let merge_preview = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_preview", "from": derived_branch.clone(), "doc": doc.clone(), "limit": 200, "dry_run": true, "merge_to_base": true } } }
    }));
    let merge_preview_text = extract_tool_text(&merge_preview);
    assert_eq!(
        merge_preview_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let diff_summary = merge_preview_text
        .get("result")
        .and_then(|v| v.get("diff_summary"))
        .expect("diff_summary");
    assert!(
        diff_summary
            .get("nodes_changed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0,
        "diff_summary should report changed nodes"
    );
    let conflict_ids = merge_preview_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert_eq!(conflict_ids.len(), 1, "expected one conflict_id in dry_run");
    let conflicts = merge_preview_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert_eq!(conflicts.len(), 1, "expected conflict preview in dry_run");
    let status = conflicts[0]
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(status, "preview", "dry_run conflicts should be previews");

    let conflicts_list = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflicts", "args": { "workspace": "ws_graph_preview", "into": base_branch.clone(), "doc": doc.clone(), "limit": 50 } } }
    }));
    let conflicts_list_text = extract_tool_text(&conflicts_list);
    let conflicts = conflicts_list_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert!(conflicts.is_empty(), "dry_run must not create conflicts");
}

#[test]
fn branchmind_graph_merge_resolved_conflict_does_not_resurface() {
    let mut server = Server::start_initialized("branchmind_graph_merge_resolved");

    let created_plan = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_resolved", "kind": "plan", "title": "Plan A" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_resolved", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let node_id = "n1";
    let apply_initial = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_resolved", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Initial title" } ] } } }
    }));
    let apply_initial_text = extract_tool_text(&apply_initial);
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

    let derived_branch = format!("{base_branch}/graph_resolved");
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_graph_resolved", "name": derived_branch.clone(), "from": base_branch.clone() } } }
    }));

    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_resolved", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Base title" } ] } } }
    }));
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_resolved", "branch": derived_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Derived title" } ] } } }
    }));

    let merge_first = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_resolved", "from": derived_branch.clone(), "doc": doc.clone(), "limit": 200, "dry_run": false, "merge_to_base": true } } }
    }));
    let merge_first_text = extract_tool_text(&merge_first);
    assert_eq!(
        merge_first_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict_ids = merge_first_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert_eq!(conflict_ids.len(), 1, "expected conflict_id on first merge");
    let conflict_id = conflict_ids[0]
        .as_str()
        .expect("conflict_id string")
        .to_string();

    // Prefer `use_into`: it resolves the conflict without applying changes (by design),
    // so the merge algorithm would re-detect divergence unless we honor the resolved status.
    let resolve = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.resolve", "args": { "workspace": "ws_graph_resolved", "conflict_id": conflict_id.clone(), "resolution": "use_into" } } }
    }));
    let resolve_text = extract_tool_text(&resolve);
    assert_eq!(
        resolve_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let merge_again = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_resolved", "from": derived_branch.clone(), "doc": doc.clone(), "limit": 200, "dry_run": false, "merge_to_base": true } } }
    }));
    let merge_again_text = extract_tool_text(&merge_again);
    assert_eq!(
        merge_again_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict_ids = merge_again_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert!(
        conflict_ids.is_empty(),
        "resolved conflicts must not re-surface in subsequent merges"
    );
    let conflicts = merge_again_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert!(
        conflicts.is_empty(),
        "resolved conflicts must not re-surface in subsequent merges"
    );

    let conflicts_list = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflicts", "args": { "workspace": "ws_graph_resolved", "into": base_branch.clone(), "doc": doc.clone(), "limit": 50 } } }
    }));
    let conflicts_list_text = extract_tool_text(&conflicts_list);
    let conflicts = conflicts_list_text
        .get("result")
        .and_then(|v| v.get("conflicts"))
        .and_then(|v| v.as_array())
        .expect("conflicts");
    assert!(
        conflicts.is_empty(),
        "graph_conflicts (default open) must not show resolved conflicts"
    );
}

#[test]
fn branchmind_graph_merge_use_from_does_not_resurface_due_to_merge_meta() {
    let mut server = Server::start_initialized("branchmind_graph_merge_use_from_no_resurface");

    let created_plan = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_use_from", "kind": "plan", "title": "Plan A" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_graph_use_from", "kind": "task", "parent": plan_id, "title": "Task A" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let node_id = "n1";
    let apply_initial = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_use_from", "target": task_id.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Initial title" } ] } } }
    }));
    let apply_initial_text = extract_tool_text(&apply_initial);
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

    let derived_branch = format!("{base_branch}/graph_use_from");
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_graph_use_from", "name": derived_branch.clone(), "from": base_branch.clone() } } }
    }));

    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_use_from", "branch": base_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Base title" } ] } } }
    }));
    server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.apply", "args": { "workspace": "ws_graph_use_from", "branch": derived_branch.clone(), "doc": doc.clone(), "ops": [ { "op": "node_upsert", "id": node_id, "type": "idea", "title": "Derived title" } ] } } }
    }));

    let merge_first = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_use_from", "from": derived_branch.clone(), "doc": doc.clone(), "limit": 200, "dry_run": false, "merge_to_base": true } } }
    }));
    let merge_first_text = extract_tool_text(&merge_first);
    assert_eq!(
        merge_first_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict_ids = merge_first_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert_eq!(conflict_ids.len(), 1, "expected conflict_id on first merge");
    let conflict_id = conflict_ids[0]
        .as_str()
        .expect("conflict_id string")
        .to_string();

    // `use_from` applies the from-branch version into the destination branch.
    // It also stamps `_merge` metadata, which must not cause a spurious conflict on the next run.
    let resolve = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.conflict.resolve", "args": { "workspace": "ws_graph_use_from", "conflict_id": conflict_id.clone(), "resolution": "use_from" } } }
    }));
    let resolve_text = extract_tool_text(&resolve);
    assert_eq!(
        resolve_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let merge_again = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.merge", "args": { "workspace": "ws_graph_use_from", "from": derived_branch.clone(), "doc": doc.clone(), "limit": 200, "dry_run": false, "merge_to_base": true } } }
    }));
    let merge_again_text = extract_tool_text(&merge_again);
    assert_eq!(
        merge_again_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflict_ids = merge_again_text
        .get("result")
        .and_then(|v| v.get("conflict_ids"))
        .and_then(|v| v.as_array())
        .expect("conflict_ids");
    assert!(
        conflict_ids.is_empty(),
        "use_from should not re-surface due to _merge metadata"
    );
    let conflicts_created = merge_again_text
        .get("result")
        .and_then(|v| v.get("conflicts_created"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        conflicts_created, 0,
        "no new conflict rows should be created after resolving with use_from"
    );
}
