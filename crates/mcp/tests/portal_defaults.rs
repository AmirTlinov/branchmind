#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn portal_defaults_in_daily_toolset_are_low_noise() {
    let mut server = Server::start_initialized_with_args(
        "portal_defaults_in_daily_toolset_are_low_noise",
        &["--toolset", "daily", "--workspace", "ws_portal_defaults"],
    );

    let status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "status",
            "arguments": {}
        }
    }));
    assert_eq!(
        status
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "portal call must not be an MCP error"
    );
    let status_result = extract_tool_text_str(&status);
    assert!(
        status_result.contains("ready checkout="),
        "daily portal status should keep the primary readiness line"
    );
    assert!(
        status_result.contains("version="),
        "status line should include a semver-ish server version for quick diagnostics"
    );
    assert!(
        !status_result.trim_start().starts_with('{'),
        "daily portal status must not fall back to JSON envelopes"
    );
    assert!(
        !status_result.contains("WATERMARK:") && !status_result.contains("ANSWER:"),
        "daily portal status should not include line protocol prefixes for content lines"
    );
    assert!(
        status_result.contains("cmd=tasks.snapshot"),
        "daily portal status must advertise a low-noise next action entrypoint"
    );

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "plan_title": "Portal Defaults Plan",
                "task_title": "Portal Defaults Task",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    assert_eq!(
        start
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "start call must not be an MCP error"
    );

    let start_result = extract_tool_text_str(&start);
    assert!(
        start_result.contains("focus TASK-"),
        "daily portal must include a focus line"
    );
    assert!(
        !start_result.trim_start().starts_with('{'),
        "daily portal must not fall back to JSON envelopes"
    );
    assert!(
        !start_result.contains("WATERMARK:") && !start_result.contains("ANSWER:"),
        "daily portal must keep content lines unprefixed"
    );
    assert!(
        start_result.contains("cmd=tasks.macro.close.step"),
        "daily portal must provide a low-noise next action command"
    );
    assert!(
        start_result.contains("\"checkpoints\":\"gate\""),
        "portal next action must be copy/paste-safe (default checkpoints=gate)"
    );
    assert!(
        !start_result.contains("workspace="),
        "daily portal must not leak workspace into action command args when default workspace is configured"
    );

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": {}
        }
    }));
    assert_eq!(
        snapshot
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "snapshot call must not be an MCP error"
    );

    let snapshot_result = extract_tool_text_str(&snapshot);
    assert!(
        snapshot_result.contains("focus TASK-"),
        "daily portal snapshot must include a focus line"
    );
    assert!(
        !snapshot_result.trim_start().starts_with('{'),
        "daily portal snapshot must not fall back to JSON envelopes"
    );
    assert!(
        !snapshot_result.contains("WATERMARK:") && !snapshot_result.contains("ANSWER:"),
        "daily snapshot must keep content lines unprefixed"
    );
    assert!(
        snapshot_result.contains("cmd=tasks.macro.close.step"),
        "daily snapshot must provide a low-noise next action command"
    );
    assert!(
        snapshot_result.contains("\"checkpoints\":\"gate\""),
        "snapshot next action must be copy/paste-safe (default checkpoints=gate)"
    );
    assert!(
        !snapshot_result.contains("workspace="),
        "snapshot action command must omit workspace when default workspace is configured"
    );

    // Budget warnings should be rendered as WARNING: lines (not ERROR:), and line protocol must stay compact.
    let snapshot_tiny = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": { "max_chars": 50 }
        }
    }));
    assert_eq!(
        snapshot_tiny
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "tiny snapshot call must not be an MCP error"
    );
    let snapshot_tiny_result = extract_tool_text_str(&snapshot_tiny);
    assert!(
        snapshot_tiny_result.contains("WARNING: BUDGET_"),
        "budget warnings must render as WARNING: lines"
    );
    assert!(
        !snapshot_tiny_result.contains("ERROR: BUDGET_"),
        "budget warnings must not render as ERROR: lines"
    );
    assert!(
        !snapshot_tiny_result.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}

#[test]
fn portal_recovery_no_focus_prefers_portals_over_full_disclosure() {
    let mut server = Server::start_initialized_with_args(
        "portal_recovery_no_focus_prefers_portals_over_full_disclosure",
        &["--toolset", "daily"],
    );

    // Empty workspace: calling a progress portal without focus should guide the agent toward the
    // correct daily portal (macro_start) rather than forcing toolset expansion.
    let close_without_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "workspace": "ws_portal_no_focus_empty", "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&close_without_focus);
    assert!(
        text.contains("ERROR: INVALID_INPUT"),
        "should return a typed invalid-input error when no focus/target exists"
    );
    assert!(
        !text.contains("expected valid input"),
        "portal recovery errors should not wrap messages in generic 'expected valid input' boilerplate"
    );
    assert_eq!(
        text.matches("| fix:").count(),
        1,
        "portal recovery errors should include at most one dedicated fix section"
    );
    assert!(
        text.contains("tasks_macro_start"),
        "should suggest the portal start macro first"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "empty-workspace recovery must not force full toolset disclosure"
    );

    // Non-empty workspace: if tasks exist but focus is missing, suggest snapshot entries for
    // known tasks so the agent can choose a target without immediately expanding into tasks_context.
    let _t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": { "workspace": "ws_portal_no_focus_tasks", "task_title": "T1" }
        }
    }));
    let _t2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": { "workspace": "ws_portal_no_focus_tasks", "task_title": "T2" }
        }
    }));
    let _cleared = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_clear", "arguments": { "workspace": "ws_portal_no_focus_tasks" } }
    }));

    let close_missing_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "workspace": "ws_portal_no_focus_tasks", "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&close_missing_focus);
    assert!(
        text.contains("ERROR: INVALID_INPUT"),
        "should still be a typed invalid-input error when focus is missing"
    );
    assert!(
        !text.contains("expected valid input"),
        "portal recovery errors should not wrap messages in generic 'expected valid input' boilerplate"
    );
    assert_eq!(
        text.matches("| fix:").count(),
        1,
        "portal recovery errors should include at most one dedicated fix section"
    );
    assert!(
        text.contains("tasks_snapshot"),
        "should suggest portal snapshots for known tasks"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "no-focus recovery should not force full toolset disclosure"
    );
}

#[test]
fn portal_disclosure_commands_use_args_hint_for_hidden_actions() {
    let mut server = Server::start_initialized_with_args(
        "portal_disclosure_commands_use_args_hint_for_hidden_actions",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_portal_disclosure_hint",
        ],
    );

    // Create a task with no steps to force the capsule to recommend tasks_decompose,
    // which is hidden in the daily toolset.
    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "workspace": "ws_portal_disclosure_hint", "kind": "plan", "title": "Plan" }
        }
    }));
    assert_eq!(
        created_plan
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "plan create must not be an MCP error"
    );
    let created_plan_result = extract_tool_text(&created_plan);
    let plan_id = created_plan_result
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("created_plan.id");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "workspace": "ws_portal_disclosure_hint", "kind": "task", "parent": plan_id, "title": "No Steps Yet" }
        }
    }));
    assert_eq!(
        created
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "task create must not be an MCP error"
    );
    let created_result = extract_tool_text(&created);
    let task_id = created_result
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("created_task.id");

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": { "task": task_id, "fmt": "lines" }
        }
    }));
    assert_eq!(
        snapshot
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(false),
        "snapshot call must not be an MCP error"
    );

    let text = extract_tool_text_str(&snapshot);
    assert!(
        text.lines()
            .any(|l| l.starts_with("tasks ") && l.contains("cmd=tasks.plan.decompose")),
        "hidden capsule action should provide a fast decompose command via the tasks portal"
    );
    assert!(
        text.contains("\"task\"") && text.contains("\"steps\""),
        "hidden capsule action must include task + steps args so the agent can copy/paste the next command"
    );
    assert!(
        !text.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}

#[test]
fn portal_recovery_unknown_id_injects_snapshot_and_start() {
    let mut server = Server::start_initialized_with_args(
        "portal_recovery_unknown_id_injects_snapshot_and_start",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_portal_unknown_id_recovery",
        ],
    );

    let snapshot_unknown = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": { "task": "TASK-999", "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&snapshot_unknown);
    assert!(
        text.contains("ERROR: UNKNOWN_ID"),
        "should return a typed unknown-id error"
    );
    assert!(
        text.contains("cmd=tasks.snapshot"),
        "should offer a portal snapshot recovery command"
    );
    assert!(
        text.contains("cmd=tasks.macro.start"),
        "should offer a safe portal fallback to re-establish focus"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "unknown-id recovery must not force full toolset disclosure"
    );
    assert!(
        !text.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}

#[test]
fn hidden_tasks_unknown_id_still_gets_portal_recovery() {
    let mut server = Server::start_initialized_with_args(
        "hidden_tasks_unknown_id_still_gets_portal_recovery",
        &["--toolset", "daily", "--workspace", "ws_hidden_unknown_id"],
    );

    // Call a hidden tool (tasks_done) with an unknown task id. The server should inject
    // portal recovery suggestions even though the tool itself is not a portal.
    let done_unknown = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_done",
            "arguments": { "task": "TASK-999", "workspace": "ws_hidden_unknown_id", "step_id": "STEP-00000001", "checkpoints": "gate", "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&done_unknown);
    assert!(
        text.contains("ERROR: UNKNOWN_ID"),
        "should return a typed unknown-id error"
    );
    assert!(
        text.contains("cmd=tasks.snapshot"),
        "should provide a portal snapshot recovery command"
    );
    assert!(
        text.contains("cmd=tasks.macro.start"),
        "should provide a portal fallback to restore focus"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "unknown-id recovery should not force toolset disclosure"
    );
}

#[test]
fn portal_recovery_macro_close_step_plan_target_suggests_macro_start() {
    let mut server = Server::start_initialized_with_args(
        "portal_recovery_macro_close_step_plan_target_suggests_macro_start",
        &["--toolset", "daily", "--workspace", "ws_portal_close_plan"],
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_create",
            "arguments": { "workspace": "ws_portal_close_plan", "kind": "plan", "title": "Plan" }
        }
    }));
    let created_plan_json = extract_tool_text(&created_plan);
    let plan_id = created_plan_json
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("tasks_create plan result.id");

    let close_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_close_step",
            "arguments": { "plan": plan_id, "fmt": "lines" }
        }
    }));
    let text = extract_tool_text_str(&close_plan);
    assert!(
        text.contains("ERROR: INVALID_INPUT"),
        "should be a typed invalid-input error for plan targets"
    );
    assert!(
        text.contains("cmd=tasks.macro.start")
            && text.contains("\"plan\"")
            && text.contains("\"task_title\""),
        "should suggest creating a task under the plan via the portal"
    );
    assert!(
        !text.contains("tools/list") && !text.contains("tasks_context"),
        "plan-target recovery must not force full toolset disclosure"
    );
    assert!(
        !text.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}

#[test]
fn portal_recovery_unknown_template_maps_to_templates_list() {
    let mut server = Server::start_initialized_with_args(
        "portal_recovery_unknown_template_maps_to_templates_list",
        &[
            "--toolset",
            "daily",
            "--workspace",
            "ws_portal_template_list",
        ],
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "plan_title": "Plan",
                "task_title": "T",
                "template": "does-not-exist",
                "fmt": "lines"
            }
        }
    }));
    let text = extract_tool_text_str(&started);
    assert!(
        text.contains("ERROR: UNKNOWN_ID"),
        "unknown template must return typed unknown-id error"
    );
    assert!(
        text.contains("tasks_templates_list"),
        "unknown template recovery should point to the templates list rather than hidden tools"
    );
    assert!(
        !text.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}

#[test]
fn portal_invalid_target_normalization_still_renders_lines() {
    let mut server = Server::start_initialized_with_args(
        "portal_invalid_target_normalization_still_renders_lines",
        &["--toolset", "daily", "--workspace", "ws_portal_bad_target"],
    );

    // When the agent passes a malformed target alias, the server normalizes the target before
    // tool dispatch. That path must still go through the portal post-processing so it doesn't
    // regress to a JSON envelope in daily/core DX.
    let bad_target = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_snapshot",
            "arguments": { "target": "task_DOES_NOT_EXIST", "fmt": "lines" }
        }
    }));
    assert_eq!(
        bad_target
            .get("result")
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "invalid target should be surfaced as a tool error (typed), not a transport error"
    );

    let text = extract_tool_text_str(&bad_target);
    assert!(
        !text.trim_start().starts_with('{'),
        "target normalization errors must render in BM-L1 lines"
    );
    assert!(
        !text.contains("WATERMARK:") && !text.contains("ANSWER:"),
        "line protocol should omit prefixes for content lines"
    );
    assert!(
        text.contains("ERROR: INVALID_INPUT"),
        "target normalization should return a typed invalid-input error"
    );
    assert!(
        !text.trim_start().starts_with('{'),
        "daily/core DX must not fall back to a JSON envelope for preprocessing errors"
    );
    assert!(
        !text.contains("\n\n"),
        "line protocol must not include empty lines"
    );
}
