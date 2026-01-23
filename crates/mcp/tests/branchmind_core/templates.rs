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
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "workspace": "ws_note_tpl",
                "name": "initiative/template",
                "template": "initiative",
                "goal": "Make the tool indispensable for long projects"
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&macro_note).starts_with("ERROR:"),
        "macro_branch_note portal must succeed"
    );

    let branches = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branch_list", "arguments": { "workspace": "ws_note_tpl" } }
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
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "workspace": "ws_note_only",
                "name": "alt",
                "content": "seed"
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&seed).starts_with("ERROR:"),
        "seed macro_branch_note must succeed"
    );

    let branches = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "branch_list", "arguments": { "workspace": "ws_note_only" } }
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
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "workspace": "ws_note_only",
                "from": "main",
                "content": "hello"
            }
        }
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
    let status_text = extract_tool_text_str(&status);
    assert!(
        status_text
            .lines()
            .next()
            .unwrap_or("")
            .contains("checkout=main"),
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
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "name": "seed",
                "content": "seed"
            }
        }
    }));

    let bad = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "from": "does-not-exist",
                "content": "note-only with checkout switch smoke"
            }
        }
    }));

    let text = extract_tool_text_str(&bad);
    assert!(
        text.contains("ERROR: UNKNOWN_ID") && text.contains("Unknown branch"),
        "should return a typed unknown-branch error"
    );
    assert!(
        text.contains("checkout=\""),
        "recovery should mention checkout to stay copy/paste-friendly"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("branch_list"),
        "daily recovery must not require progressive disclosure for a simple note"
    );
    assert!(
        text.contains("macro_branch_note content=\"note-only with checkout switch smoke\""),
        "recovery must include a copy/paste-safe retry command (with quoting)"
    );
}

#[test]
fn branchmind_macro_branch_note_reuses_existing_branch_when_name_exists() {
    let mut server = Server::start_initialized("branchmind_note_reuse_existing_branch");

    let first = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "workspace": "ws_note_reuse",
                "name": "initiative/reuse",
                "content": "first"
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&first).starts_with("ERROR:"),
        "first macro_branch_note must succeed"
    );
    let first_text = extract_tool_text_str(&first);
    assert!(
        first_text
            .lines()
            .next()
            .unwrap_or("")
            .starts_with("branch initiative/reuse"),
        "first call should create the branch (line protocol: starts with `branch ...`)"
    );

    let second = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "macro_branch_note",
            "arguments": {
                "workspace": "ws_note_reuse",
                "name": "initiative/reuse",
                "content": "second"
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&second).starts_with("ERROR:"),
        "second macro_branch_note must succeed even when the branch already exists"
    );
    let second_text = extract_tool_text_str(&second);
    assert!(
        second_text.contains("note committed") && second_text.contains("initiative/reuse"),
        "second call should append a note on the existing branch (no conflict)"
    );
    assert!(
        !second_text
            .lines()
            .next()
            .unwrap_or("")
            .starts_with("branch "),
        "second call must not claim it created a new branch"
    );
}
