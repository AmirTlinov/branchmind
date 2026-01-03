#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_diff_and_merge_notes_smoke() {
    let mut server = Server::start_initialized("branchmind_diff_and_merge_notes_smoke");

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

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws1", "task": task_id.clone() } }
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

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "checkout", "arguments": { "workspace": "ws1", "ref": canonical_branch.clone() } }
    }));

    let base_note_content = "base note";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws1", "target": task_id.clone(), "content": base_note_content } }
    }));

    let derived_branch = format!("{}/alt2", canonical_branch);
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "branch_create", "arguments": { "workspace": "ws1", "name": derived_branch.clone() } }
    }));

    let derived_note_content = "derived note";
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws1", "branch": derived_branch.clone(), "doc": notes_doc.clone(), "content": derived_note_content } }
    }));

    let diff = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "diff", "arguments": { "workspace": "ws1", "from": canonical_branch.clone(), "to": derived_branch.clone(), "doc": notes_doc.clone(), "limit": 50 } }
    }));
    let diff_text = extract_tool_text(&diff);
    assert_eq!(
        diff_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let diff_entries = diff_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("diff entries");
    assert!(
        diff_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content)),
        "diff(base→derived) must include derived note"
    );
    assert!(
        !diff_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(base_note_content)),
        "diff(base→derived) must not include base note"
    );

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "merge", "arguments": { "workspace": "ws1", "from": derived_branch.clone(), "into": canonical_branch.clone(), "doc": notes_doc.clone() } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let merged_count = merge_text
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(merged_count, 1, "first merge must merge exactly one note");

    let merge2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "merge", "arguments": { "workspace": "ws1", "from": derived_branch.clone(), "into": canonical_branch.clone(), "doc": notes_doc.clone() } }
    }));
    let merge2_text = extract_tool_text(&merge2);
    assert_eq!(
        merge2_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let merged_count2 = merge2_text
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_u64())
        .unwrap_or(999);
    assert_eq!(
        merged_count2, 0,
        "second merge must be idempotent (merged=0)"
    );

    let show_base = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws1", "branch": canonical_branch, "doc": notes_doc, "limit": 50 } }
    }));
    let show_base_text = extract_tool_text(&show_base);
    let base_entries = show_base_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        base_entries
            .iter()
            .any(|e| e.get("content").and_then(|v| v.as_str()) == Some(derived_note_content)),
        "base view must include merged derived note after merge"
    );
}
