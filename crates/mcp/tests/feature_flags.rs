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
fn knowledge_commands_are_removed_from_surface() {
    let mut server = Server::start_initialized_with_args(
        "knowledge_commands_are_removed_from_surface",
        &["--toolset", "daily", "--workspace", "ws_flags_no_knowledge"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.knowledge.upsert", "args": {
            "anchor": "core",
            "key": "determinism",
            "card": { "title": "Invariant", "text": "Removed feature check" }
        } } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        !out.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
        "expected failure for removed command: {out}"
    );
    assert_eq!(
        out.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "UNKNOWN_CMD",
        "expected UNKNOWN_CMD for removed knowledge command: {out}"
    );
    let recovery = out
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        recovery.contains("knowledge removed by design"),
        "expected explicit removed-knowledge recovery hint: {out}"
    );

    let schema_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "schema.get", "args": {
            "cmd": "think.knowledge.recall"
        } } }
    }));
    let schema_out = extract_tool_text(&schema_resp);
    assert!(
        !schema_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "expected failure for removed schema target: {schema_out}"
    );
    assert_eq!(
        schema_out
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "UNKNOWN_CMD",
        "expected UNKNOWN_CMD for removed schema target: {schema_out}"
    );
    let schema_recovery = schema_out
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        schema_recovery.contains("knowledge removed by design"),
        "expected explicit removed-knowledge recovery hint in schema.get: {schema_out}"
    );

    let add_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.knowledge", "args": {
            "text": "legacy removed"
        } } }
    }));
    let add_out = extract_tool_text(&add_resp);
    assert!(
        !add_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "expected failure for removed think.add.knowledge command: {add_out}"
    );
    assert_eq!(
        add_out
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "UNKNOWN_CMD",
        "expected UNKNOWN_CMD for think.add.knowledge: {add_out}"
    );
    let add_recovery = add_out
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        add_recovery.contains("knowledge removed by design"),
        "expected explicit removed-knowledge recovery hint in think.add.knowledge: {add_out}"
    );
}

#[test]
fn slice_plans_v1_flag_can_disable_slice_tools() {
    let mut server = Server::start_initialized_with_args(
        "slice_plans_v1_flag_can_disable_slice_tools",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flags_slices",
            "--no-slice-plans-v1",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.apply", "args": {} } }
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
fn jobs_slice_first_fail_closed_flag_can_allow_legacy_scout_dispatch() {
    let mut server = Server::start_initialized_with_args(
        "jobs_slice_first_fail_closed_flag_can_allow_legacy_scout_dispatch",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_flags_jobs_legacy",
            "--no-jobs-slice-first-fail-closed",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.macro.dispatch.scout", "args": {
            "task": "PLAN-LEGACY",
            "slice_id": "SLC-LEGACY-001",
            "objective": "Legacy scout objective",
            "dry_run": true
        } } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "expected success: {out}"
    );
}
