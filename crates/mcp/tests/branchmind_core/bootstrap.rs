#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_bootstrap_defaults() {
    let mut server = Server::start_initialized("branchmind_bootstrap_defaults");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_boot" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let branch_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.list", "args": { "workspace": "ws_boot", "limit": 50 } } }
    }));
    let branch_list_text = extract_tool_text(&branch_list);
    let branches = branch_list_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .expect("branches");
    let has_main = branches
        .iter()
        .any(|b| b.get("name").and_then(|v| v.as_str()) == Some("main"));
    assert!(has_main, "default branch main should exist");

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.notes.commit", "args": { "workspace": "ws_boot", "content": "hello" } } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        note_text
            .get("result")
            .and_then(|v| v.get("entry"))
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        note_text
            .get("result")
            .and_then(|v| v.get("entry"))
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some("notes")
    );

    let show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.show", "args": { "workspace": "ws_boot", "doc_kind": "notes", "limit": 10 } } }
    }));
    let show_text = extract_tool_text(&show);
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some("notes")
    );
}

#[test]
fn branchmind_branch_list_sets_truncated_when_limit_hides_items() {
    let mut server = Server::start_initialized("branchmind_branch_list_truncated");

    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_branch_list_trunc" } } }
    }));

    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_branch_list_trunc", "name": "alpha" } } }
    }));
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_branch_list_trunc", "name": "beta" } } }
    }));

    let branch_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.list", "args": { "workspace": "ws_branch_list_trunc", "limit": 1 } } }
    }));
    let branch_list_text = extract_tool_text(&branch_list);

    let branches = branch_list_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .expect("branches");
    assert_eq!(
        branches.len(),
        1,
        "limit=1 should return exactly one branch"
    );
    assert_eq!(
        branch_list_text
            .get("result")
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64()),
        Some(1),
        "count should match the returned branches length"
    );
    assert_eq!(
        branch_list_text
            .get("result")
            .and_then(|v| v.get("truncated"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "truncated should be true when more branches exist beyond the limit"
    );
}
