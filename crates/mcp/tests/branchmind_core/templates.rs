#![forbid(unsafe_code)]

use super::support::*;
use serde_json::json;

#[test]
fn branchmind_macro_branch_note_supports_template() {
    let mut server = Server::start_initialized("branchmind_note_templates");

    let macro_note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "workspace": "ws_note_tpl",
                "name": "initiative/template",
                "template": "initiative",
                "goal": "Make the tool indispensable for long projects"
            } } }
    }));
    assert!(
        !extract_tool_text_str(&macro_note).starts_with("ERROR:"),
        "macro_branch_note portal must succeed"
    );

    let branches = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.list", "args": { "workspace": "ws_note_tpl" } } }
    }));
    let branches_text = extract_tool_text(&branches);
    let has_branch = branches_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .any(|b| b.get("name").and_then(|v| v.as_str()) == Some("initiative/template"))
        })
        .unwrap_or(false);
    assert!(
        has_branch,
        "macro_branch_note should create the named branch"
    );
}

#[test]
fn branchmind_macro_branch_note_can_append_without_creating_branch() {
    let mut server = Server::start_initialized("branchmind_note_without_branch");

    // Create a branch first so we can verify `from` acts as a checkout switch in note-only mode.
    let seed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "workspace": "ws_note_only",
                "name": "alt",
                "content": "seed"
            } } }
    }));
    assert!(
        !extract_tool_text_str(&seed).starts_with("ERROR:"),
        "seed macro_branch_note must succeed"
    );

    let branches = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.list", "args": { "workspace": "ws_note_only" } } }
    }));
    let branches_text = extract_tool_text(&branches);
    let has_alt = branches_text
        .get("result")
        .and_then(|v| v.get("branches"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .any(|b| b.get("name").and_then(|v| v.as_str()) == Some("alt"))
        })
        .unwrap_or(false);
    assert!(
        has_alt,
        "seed macro_branch_note should create the alt branch"
    );

    // Now append a note without creating a new branch, switching checkout to `main` via `from`.
    let note_only = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "workspace": "ws_note_only",
                "from": "main",
                "content": "hello"
            } } }
    }));
    assert!(
        !extract_tool_text_str(&note_only).starts_with("ERROR:"),
        "note-only macro_branch_note must succeed"
    );

    let status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": "ws_note_only" } }
    }));
    let status_json = extract_tool_text(&status);
    assert_eq!(
        status_json.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "status portal must succeed"
    );
    assert_eq!(
        status_json
            .get("result")
            .and_then(|v| v.get("checkout"))
            .and_then(|v| v.as_str()),
        Some("main"),
        "note-only mode should switch checkout to main via from=main"
    );
}

#[test]
fn branchmind_macro_branch_note_unknown_from_is_recoverable_in_full() {
    let mut server = Server::start_initialized_with_args(
        "branchmind_note_unknown_from_is_recoverable_in_full",
        &["--toolset", "full", "--workspace", "ws_note_unknown_from"],
    );

    // Ensure a checkout exists for the workspace.
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "name": "seed",
                "content": "seed"
            } } }
    }));

    let bad = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "from": "does-not-exist",
                "content": "note-only with checkout switch smoke"
            } } }
    }));

    let out = extract_tool_text(&bad);
    assert_eq!(
        out.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "bad macro_branch_note must fail"
    );
    assert_eq!(
        out.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ID"),
        "should return a typed unknown-branch error"
    );
    assert!(
        out.get("error")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("Unknown branch"),
        "unknown-branch message should be present"
    );
    assert!(
        out.get("error")
            .and_then(|v| v.get("recovery"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("checkout=\""),
        "recovery should mention checkout to stay copy/paste-friendly"
    );

    let actions = out
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions
            .iter()
            .any(|a| a.get("tool").and_then(|v| v.as_str()) == Some("think")),
        "recovery should include a copy/paste retry action"
    );
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("think")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("think.idea.branch.create")
                && a.get("args")
                    .and_then(|v| v.get("args"))
                    .and_then(|v| v.get("content"))
                    .and_then(|v| v.as_str())
                    == Some("note-only with checkout switch smoke")
        }),
        "retry action must include the original content"
    );
}

#[test]
fn branchmind_macro_branch_note_reuses_existing_branch_when_name_exists() {
    let mut server = Server::start_initialized("branchmind_note_reuse_existing_branch");

    let first = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "workspace": "ws_note_reuse",
                "name": "initiative/reuse",
                "content": "first"
            } } }
    }));
    assert!(
        !extract_tool_text_str(&first).starts_with("ERROR:"),
        "first macro_branch_note must succeed"
    );
    let first_json = extract_tool_text(&first);
    assert_eq!(
        first_json.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "first macro_branch_note must succeed"
    );
    assert_eq!(
        first_json
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.get("created"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "first call should create the branch"
    );

    let second = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.idea.branch.create", "args": {
                "workspace": "ws_note_reuse",
                "name": "initiative/reuse",
                "content": "second"
            } } }
    }));
    assert!(
        !extract_tool_text_str(&second).starts_with("ERROR:"),
        "second macro_branch_note must succeed even when the branch already exists"
    );
    let second_json = extract_tool_text(&second);
    assert_eq!(
        second_json.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "second macro_branch_note must succeed even when the branch already exists"
    );
    assert_eq!(
        second_json
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.get("created"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "second call must not claim it created a new branch"
    );
}
