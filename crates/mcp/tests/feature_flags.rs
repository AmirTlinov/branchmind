#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn ux_proof_v2_flag_can_disable_proof_input() {
    let mut server = Server::start_initialized_with_args(
        "ux_proof_v2_flag_can_disable_proof_input",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flags_proof",
            "--no-ux-proof-v2",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
            "proof_input": "cargo test -q"
        } } }
    }));
    let text = extract_tool_text_str(&resp);
    assert!(
        text.lines()
            .next()
            .is_some_and(|l| l.starts_with("ERROR: FEATURE_DISABLED")),
        "expected FEATURE_DISABLED when --no-ux-proof-v2 is set: {text}"
    );
}

#[test]
fn note_promote_flag_can_disable_note_promote_cmd() {
    let mut server = Server::start_initialized_with_args(
        "note_promote_flag_can_disable_note_promote_cmd",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flags_note",
            "--no-note-promote",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.note.promote", "args": {} } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        !out.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
        "expected failure: {out}"
    );
    assert_eq!(
        out.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "FEATURE_DISABLED",
        "expected FEATURE_DISABLED: {out}"
    );
}

#[test]
fn knowledge_autolint_flag_downgrades_auto_lint_to_warning() {
    let mut server = Server::start_initialized_with_args(
        "knowledge_autolint_flag_downgrades_auto_lint_to_warning",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flags_kb",
            "--no-knowledge-autolint",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.knowledge.upsert", "args": {
            "anchor": "core",
            "key": "determinism",
            "lint_mode": "auto",
            "card": { "title": "Determinism invariants", "text": "Claim: ...\nScope: core\nApply: ...\nProof: CMD: make check\nExpiry: 2027-01-01" }
        } } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "expected success: {out}"
    );
    let warnings = out
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| { w.get("code").and_then(|v| v.as_str()) == Some("FEATURE_DISABLED") }),
        "expected FEATURE_DISABLED warning when auto lint is requested but disabled: {warnings:?}"
    );
}
