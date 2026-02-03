#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_branching_inherits_base_snapshot() {
    let mut server = Server::start_initialized("branchmind_branching_inherits_base_snapshot");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "plan", "title": "Plan A" } } }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws1", "kind": "task", "parent": plan_id, "title": "Task A" } } }
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
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws1", "task": task_id.clone() } } }
    }));
    let radar_text = extract_tool_text(&radar);
    let canonical_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let notes_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("notes_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.notes_doc")
        .to_string();

    let checkout = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.checkout", "args": { "workspace": "ws1", "ref": canonical_branch.clone() } } }
    }));
    let checkout_text = extract_tool_text(&checkout);
    assert_eq!(
        checkout_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        checkout_text
            .get("result")
            .and_then(|v| v.get("current"))
            .and_then(|v| v.as_str()),
        Some(canonical_branch.as_str())
    );

    let base_note_content = "base note";
    let base_note_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws1", "target": task_id.clone(), "content": base_note_content } } }
    }));
    let base_note_commit_text = extract_tool_text(&base_note_commit);
    assert_eq!(
        base_note_commit_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let derived_branch = format!("{}/alt", canonical_branch);
    let branch_create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws1", "name": derived_branch.clone() } } }
    }));
    let branch_create_text = extract_tool_text(&branch_create);
    assert_eq!(
        branch_create_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        branch_create_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.get("base_branch"))
            .and_then(|v| v.as_str()),
        Some(canonical_branch.as_str())
    );

    let derived_note_content = "derived note";
    let derived_note_commit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "content": derived_note_content } } }
    }));
    let derived_note_commit_text = extract_tool_text(&derived_note_commit);
    assert_eq!(
        derived_note_commit_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let show_derived = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.show", "args": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } } }
    }));
    let derived_text = extract_tool_text(&show_derived);
    let derived_entries = derived_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        derived_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(base_note_content) }),
        "derived view must include base note"
    );
    assert!(
        derived_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content) }),
        "derived view must include derived note"
    );

    let show_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.show", "args": { "workspace": "ws1", "branch": canonical_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } } }
    }));
    let base_text = extract_tool_text(&show_base);
    let base_entries = base_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        base_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(base_note_content)),
        "base view must include base note"
    );
    assert!(
        !base_entries
            .iter()
            .any(|e| { e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content) }),
        "base view must not include derived note"
    );

    let branch_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.list", "args": { "workspace": "ws1", "limit": 200 } } }
    }));
    let branch_list_text = extract_tool_text(&branch_list);
    let branches = branch_list_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .expect("branches");
    assert!(
        branches
            .iter()
            .any(|b| b.get("name").and_then(|v| v.as_str()) == Some(canonical_branch.as_str())),
        "branch list must include canonical branch"
    );
    assert!(
        branches
            .iter()
            .any(|b| b.get("name").and_then(|v| v.as_str()) == Some(derived_branch.as_str())),
        "branch list must include derived branch"
    );
}
