#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::{Value, json};

fn tools_call(server: &mut Server, id: i64, name: &str, arguments: Value) -> Value {
    server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    }))
}

fn assert_recovery_command_is_schema_valid(server: &mut Server, id: i64, line: &str) {
    let trimmed = line.trim();
    assert!(!trimmed.is_empty(), "empty command line");

    let (tool, args) = if trimmed.starts_with("open") {
        ("open".to_string(), parse_open_command_line(trimmed))
    } else {
        parse_portal_command_line(trimmed)
    };

    let resp = tools_call(server, id, &tool, Value::Object(args));
    assert!(
        resp.get("error").is_none(),
        "command line must not produce JSON-RPC error: {trimmed}\nresp={resp}"
    );

    let payload = extract_tool_text(&resp);
    if let Some(code) = payload
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str())
    {
        assert!(
            !matches!(code, "INVALID_INPUT" | "UNKNOWN_CMD" | "UNKNOWN_OP"),
            "recovery command must be schema-valid and dispatchable (got {code}): {trimmed}\nresp={payload}"
        );
    }
}

fn assert_all_recovery_commands_are_schema_valid(server: &mut Server, rendered: &str) {
    let cmds = extract_bm_command_lines(rendered);
    assert!(
        !cmds.is_empty(),
        "expected at least one recovery command line, got:\n{rendered}"
    );
    for (i, cmd) in cmds.iter().enumerate() {
        assert_recovery_command_is_schema_valid(server, 1000 + i as i64, cmd);
    }
}

#[test]
fn recovery_actions_copy_paste_valid_for_tasks_snapshot_no_focus() {
    let mut server = Server::start_initialized_with_args(
        "recovery_actions_copy_paste_valid_for_tasks_snapshot_no_focus",
        &["--toolset", "daily"],
    );

    let resp = tools_call(
        &mut server,
        1,
        "tasks",
        json!({
            "workspace": "ws_cp_snapshot",
            "op": "call",
            "cmd": "tasks.snapshot",
            "args": { "view": "smart" },
            "fmt": "lines"
        }),
    );
    let rendered = extract_tool_text_str(&resp);
    assert!(rendered.contains("ERROR:"), "expected an error payload");
    assert_all_recovery_commands_are_schema_valid(&mut server, &rendered);
}

#[test]
fn recovery_actions_copy_paste_valid_for_system_schema_get_invalid_input() {
    let mut server = Server::start_initialized_with_args(
        "recovery_actions_copy_paste_valid_for_system_schema_get_invalid_input",
        &["--toolset", "daily"],
    );

    // Intentionally omit args.cmd to force INVALID_INPUT + actions.
    let resp = tools_call(
        &mut server,
        1,
        "system",
        json!({
            "workspace": "ws_cp_system",
            "op": "schema.get",
            "args": {},
            "fmt": "lines"
        }),
    );
    let rendered = extract_tool_text_str(&resp);
    assert!(rendered.contains("ERROR:"), "expected an error payload");
    assert_all_recovery_commands_are_schema_valid(&mut server, &rendered);
}

#[test]
fn recovery_actions_copy_paste_valid_for_jobs_open_invalid_input() {
    let mut server = Server::start_initialized_with_args(
        "recovery_actions_copy_paste_valid_for_jobs_open_invalid_input",
        &["--toolset", "daily"],
    );

    let resp = tools_call(
        &mut server,
        1,
        "jobs",
        json!({
            "workspace": "ws_cp_jobs",
            "op": "call",
            "cmd": "jobs.open",
            "args": {},
            "fmt": "lines"
        }),
    );
    let rendered = extract_tool_text_str(&resp);
    assert!(rendered.contains("ERROR:"), "expected an error payload");
    assert_all_recovery_commands_are_schema_valid(&mut server, &rendered);
}

#[test]
fn recovery_actions_copy_paste_valid_for_deep_reasoning_gate() {
    let mut server = Server::start_initialized_with_args(
        "recovery_actions_copy_paste_valid_for_deep_reasoning_gate",
        &["--toolset", "daily"],
    );

    // Start a deep-mode task.
    let _t = tools_call(
        &mut server,
        1,
        "tasks",
        json!({
            "workspace": "ws_cp_deep",
            "op": "call",
            "cmd": "tasks.macro.start",
            "args": { "template": "flagship-task", "task_title": "Deep gate copy/paste" }
        }),
    );

    // Close step → expect deep gate error + think.card recovery commands.
    let close = tools_call(
        &mut server,
        2,
        "tasks",
        json!({
            "workspace": "ws_cp_deep",
            "op": "call",
            "cmd": "tasks.macro.close.step",
            "args": { "checkpoints": "gate" },
            "fmt": "lines"
        }),
    );
    let rendered = extract_tool_text_str(&close);
    assert!(rendered.contains("ERROR:"), "expected a gate error payload");
    assert_all_recovery_commands_are_schema_valid(&mut server, &rendered);
}

#[test]
fn recovery_actions_copy_paste_valid_for_budget_exceeded_retry() {
    let mut server = Server::start_initialized_with_args(
        "recovery_actions_copy_paste_valid_for_budget_exceeded_retry",
        &["--toolset", "daily"],
    );

    let resp = tools_call(
        &mut server,
        1,
        "think",
        json!({
            "workspace": "ws_cp_budget",
            "op": "knowledge.query",
            "args": { "limit": 12, "max_chars": 7000 },
            "budget_profile": "portal",
            "fmt": "lines"
        }),
    );
    let rendered = extract_tool_text_str(&resp);
    assert!(
        rendered.contains("ERROR:"),
        "expected a budget error payload"
    );
    assert_all_recovery_commands_are_schema_valid(&mut server, &rendered);
}
