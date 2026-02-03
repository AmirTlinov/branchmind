#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn assert_tag_light(text: &str) {
    assert!(
        !text.trim_start().starts_with('{'),
        "fmt=lines must not fall back to JSON envelopes"
    );
    assert!(
        !text.contains("WATERMARK:") && !text.contains("ANSWER:"),
        "fmt=lines must not include legacy tag prefixes for content lines"
    );
    assert!(
        !text.contains("\n\n"),
        "fmt=lines must not include empty lines"
    );
    for (idx, line) in text.lines().enumerate() {
        assert!(
            !line.trim().is_empty(),
            "fmt=lines must not include empty line at {idx}"
        );
    }
}

#[test]
fn proof_required_step_fails_portal_first_and_recovers_with_proof() {
    let mut server = Server::start_initialized_with_args(
        "proof_required_step_fails_portal_first_and_recovers_with_proof",
        &["--toolset", "daily", "--workspace", "ws_proof_required"],
    );

    // Start a principal task (principal templates include proof-required verification step).
    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Task", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    let start_text = extract_tool_text_str(&started);
    assert_tag_light(&start_text);

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(
            !text.starts_with("ERROR:"),
            "early step closure should succeed without proof"
        );
    }

    // Snapshot should proactively include a proof placeholder in the next action.
    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": {} } }
    }));
    let snap_text = extract_tool_text_str(&snapshot);
    assert_tag_light(&snap_text);
    let snap_lines = snap_text.lines().collect::<Vec<_>>();
    assert!(
        snap_lines.len() == 2,
        "snapshot must stay 2 lines (state + command) in daily flagship, got {} lines:\n{snap_text}",
        snap_lines.len()
    );
    assert!(
        snap_lines[0].contains("| ref="),
        "snapshot state line must include ref=... for navigation"
    );
    assert!(
        snap_lines[1].starts_with("think ")
            && snap_lines[1].contains("op=call")
            && snap_lines[1].contains("cmd=think.card"),
        "when anchor is missing, snapshot must suggest a canonical anchor attach command via think portal"
    );
    assert!(
        snap_lines[1].contains("v:canon"),
        "anchor attach suggestion must be canonical (v:canon)"
    );
    assert!(
        snap_lines[0].contains("| backup tasks ")
            && snap_lines[0].contains("cmd=tasks.macro.close.step"),
        "snapshot state line must preserve progress as a backup command"
    );
    assert!(
        snap_lines[0].contains("\"proof\""),
        "proof-required step must inject a proof placeholder into the backup progress command"
    );

    // Attempting to close without proof must produce a typed error with a portal-first retry.
    let close_without_proof = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
    }));
    let err_text = extract_tool_text_str(&close_without_proof);
    assert_tag_light(&err_text);
    let err_lines = err_text.lines().collect::<Vec<_>>();
    assert_eq!(err_lines.len(), 2, "proof-required error should be 2 lines");
    assert!(
        err_lines[0].starts_with("ERROR: PROOF_REQUIRED"),
        "error must be typed as PROOF_REQUIRED"
    );
    assert!(
        err_lines[1].starts_with("tasks ") && err_lines[1].contains("cmd=tasks.macro.close.step"),
        "recovery must stay portal-first (retry macro)"
    );
    assert!(
        err_lines[1].contains("proof"),
        "recovery command should include a proof placeholder"
    );

    // Providing proof should allow the macro to capture evidence and close the step.
    let close_with_proof = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "proof": "cargo test" } } }
    }));
    let ok_text = extract_tool_text_str(&close_with_proof);
    assert_tag_light(&ok_text);
    assert!(
        !ok_text.starts_with("ERROR:"),
        "macro_close_step should succeed when proof is provided"
    );
}

#[test]
fn proof_weak_warning_is_soft_and_does_not_block_closing() {
    let mut server = Server::start_initialized_with_args(
        "proof_weak_warning_is_soft_and_does_not_block_closing",
        &["--toolset", "daily", "--workspace", "ws_proof_weak"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Weak Task", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    // Provide a minimal proof (command only). The macro should auto-normalize it to a CMD receipt
    // and emit a soft PROOF_WEAK warning for the missing LINK receipt.
    let closed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "proof": "cargo test"
            } } }
    }));
    let text = extract_tool_text_str(&closed);
    assert_tag_light(&text);
    assert!(
        !text.starts_with("ERROR:"),
        "closing should not be blocked by proof lint"
    );
    assert!(
        text.lines().any(|l| l.starts_with("WARNING: PROOF_WEAK")),
        "soft proof lint warning should be surfaced"
    );
}

#[test]
fn proof_markdown_bullets_are_normalized_and_do_not_trigger_proof_weak() {
    let mut server = Server::start_initialized_with_args(
        "proof_markdown_bullets_are_normalized_and_do_not_trigger_proof_weak",
        &["--toolset", "daily", "--workspace", "ws_proof_md_bullets"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof MD Bullets", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    // Provide proof as a markdown list. This should normalize to CMD/LINK receipts and must not
    // emit a PROOF_WEAK warning for a present LINK line.
    let closed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "proof": "- cargo test\n- LINK: https://example.com/ci/run/123"
            } } }
    }));
    let text = extract_tool_text_str(&closed);
    assert_tag_light(&text);
    assert!(
        !text.starts_with("ERROR:"),
        "closure with proof should succeed"
    );
    assert!(
        !text.lines().any(|l| l.starts_with("WARNING: PROOF_WEAK")),
        "markdown bullet proofs must not trigger PROOF_WEAK when LINK is present"
    );
}

#[test]
fn proof_input_parses_and_supports_strict_policy() {
    let mut server = Server::start_initialized_with_args(
        "proof_input_parses_and_supports_strict_policy",
        &["--toolset", "daily", "--workspace", "ws_proof_input"],
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Input Task", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    for id in 2..=4 {
        let closed = server.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    let close_with_proof_input = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
            "proof_input": ["cargo test -q", "https://example.com/log"]
        } } }
    }));
    let ok_text = extract_tool_text_str(&close_with_proof_input);
    assert_tag_light(&ok_text);
    assert!(
        !ok_text.starts_with("ERROR:"),
        "macro_close_step should accept proof_input"
    );

    let mut server_strict = Server::start_initialized_with_args(
        "proof_input_strict_rejects_ambiguous",
        &["--toolset", "daily", "--workspace", "ws_proof_input_strict"],
    );
    let started = server_strict.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Input Strict", "template": "basic-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    let strict_close = server_strict.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
            "proof_input": "ran tests",
            "proof_parse_policy": "strict"
        } } }
    }));
    let err_text = extract_tool_text_str(&strict_close);
    assert_tag_light(&err_text);
    assert!(
        err_text
            .lines()
            .next()
            .is_some_and(|l| l.starts_with("ERROR: PROOF_PARSE_AMBIGUOUS")),
        "strict proof_input should fail with PROOF_PARSE_AMBIGUOUS"
    );
}

#[test]
fn proof_url_attachment_satisfies_soft_link_receipt_lint() {
    let mut server = Server::start_initialized_with_args(
        "proof_url_attachment_satisfies_soft_link_receipt_lint",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_proof_attachment_link",
        ],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Attachment Link", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    // Provide CMD in checks and the CI link via attachments.
    let closed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "proof": {
                    "checks": ["cargo test"],
                    "attachments": ["https://example.com/ci/run/456"]
                }
            } } }
    }));
    let text = extract_tool_text_str(&closed);
    assert_tag_light(&text);
    assert!(
        !text.starts_with("ERROR:"),
        "closure with proof should succeed"
    );
    assert!(
        !text.lines().any(|l| l.starts_with("WARNING: PROOF_WEAK")),
        "URL attachments should satisfy the LINK receipt for the soft lint"
    );
}

#[test]
fn proof_placeholder_is_ignored_and_does_not_satisfy_proof_required_gate() {
    let mut server = Server::start_initialized_with_args(
        "proof_placeholder_is_ignored_and_does_not_satisfy_proof_required_gate",
        &["--toolset", "daily", "--workspace", "ws_proof_placeholder"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Placeholder Task", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    // Attempt to close the proof-required step with the literal placeholder proof.
    // This must NOT satisfy the proof-required gate.
    let closed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "proof": [
                    "CMD: <fill: command you ran>",
                    "LINK: <fill: CI run / artifact / log>"
                ]
            } } }
    }));
    let text = extract_tool_text_str(&closed);
    assert_tag_light(&text);
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "error must stay 2 lines");
    assert!(
        lines[0].starts_with("ERROR: PROOF_REQUIRED"),
        "placeholder-only proof must be ignored (still PROOF_REQUIRED)"
    );
    assert!(
        lines[1].starts_with("tasks ") && lines[1].contains("cmd=tasks.macro.close.step"),
        "recovery must stay portal-first"
    );
    assert!(
        lines[1].contains("proof"),
        "recovery command must include proof template"
    );
}

#[test]
fn proof_in_note_is_salvaged_and_satisfies_proof_required_gate() {
    let mut server = Server::start_initialized_with_args(
        "proof_in_note_is_salvaged_and_satisfies_proof_required_gate",
        &["--toolset", "daily", "--workspace", "ws_proof_note_salvage"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Proof Note Salvage", "template": "principal-task", "reasoning_mode": "normal" } } }
    }));
    assert_tag_light(&extract_tool_text_str(&started));

    // Close first 3 steps (no proof required yet).
    for id in 2..=4 {
        let closed = server.request(json!( {
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {} } }
        }));
        let text = extract_tool_text_str(&closed);
        assert_tag_light(&text);
        assert!(!text.starts_with("ERROR:"), "early closure should succeed");
    }

    // Provide proof as receipts inside the note field (no explicit proof arg).
    // The macro must salvage the receipts and satisfy the proof-required gate.
    let closed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "note": "CMD: cargo test\nLINK: https://example.com/ci/run/789"
            } } }
    }));
    let text = extract_tool_text_str(&closed);
    assert_tag_light(&text);
    assert!(
        !text.starts_with("ERROR:"),
        "macro_close_step should succeed when proof receipts are present in note"
    );
    assert!(
        !text.lines().any(|l| l.starts_with("WARNING: PROOF_WEAK")),
        "salvaged proof receipts should satisfy the soft lint when both CMD+LINK are present"
    );
}
