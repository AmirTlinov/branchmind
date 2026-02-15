#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn tool_call_is_error(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|v| v.get("isError"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

#[test]
fn system_help_is_v1_and_has_no_tools_list_toolset() {
    let mut server = Server::start_initialized_with_args(
        "system_help_is_v1_and_has_no_tools_list_toolset",
        &["--workspace", "ws_help_v1"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.help", "args": {} } }
    }));

    let text = extract_tool_text_str(&resp);
    assert!(
        !text.contains("tools/list toolset"),
        "help must not teach tools/list toolset disclosure; got:\n{text}"
    );
    assert!(
        text.contains("system(op=cmd.list") && text.contains("system(op=schema.get"),
        "help should mention system(op=cmd.list) and system(op=schema.get); got:\n{text}"
    );
    assert!(
        text.contains("system(op=tools.list")
            && text.contains("system(op=tutorial")
            && text.contains("system(op=schema.list"),
        "help should mention system(op=tools.list), system(op=tutorial), system(op=schema.list); got:\n{text}"
    );
    assert!(
        text.contains("tasks(op=call cmd=tasks.macro.start)"),
        "help should reference tasks.macro.start as the golden path macro; got:\n{text}"
    );
}

#[test]
fn unknown_op_recovery_provides_ops_summary_action() {
    let mut server = Server::start_initialized_with_args(
        "unknown_op_recovery_provides_ops_summary_action",
        &["--workspace", "ws_unknown_op"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "nope", "args": {} } }
    }));

    let text = extract_tool_text(&resp);
    let code = text
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(code, Some("UNKNOWN_OP"));

    let recovery = text
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !recovery.contains("tools/list"),
        "UNKNOWN_OP recovery must not point at tools/list; got: {recovery}"
    );

    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("system")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("ops.summary")
        }),
        "UNKNOWN_OP should provide a system ops.summary recovery action; got actions={actions:?}"
    );
}

#[test]
fn system_tools_list_lists_10_tools() {
    let mut server = Server::start_initialized_with_args(
        "system_tools_list_lists_10_tools",
        &["--workspace", "ws_tools_list"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "tools.list", "args": {} } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system tools.list must succeed; got: {text}"
    );
    let tools = text
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");
    let quickstart_schema_hint = text
        .get("result")
        .and_then(|v| v.get("quickstart_schema_hint"))
        .expect("result.quickstart_schema_hint");
    assert!(
        quickstart_schema_hint.get("defaults").is_some(),
        "tools.list must include quickstart_schema_hint.defaults; got: {quickstart_schema_hint}"
    );
    assert!(
        quickstart_schema_hint.get("recipe_uses_defaults").is_some(),
        "tools.list must include quickstart_schema_hint.recipe_uses_defaults; got: {quickstart_schema_hint}"
    );
    assert_eq!(
        tools.len(),
        10,
        "tools.list must return 10 tools; got: {tools:?}"
    );
    assert!(
        tools
            .iter()
            .any(|t| t.get("tool").and_then(|v| v.as_str()) == Some("tasks")),
        "tools.list should include tasks; got: {tools:?}"
    );
    assert!(
        tools
            .iter()
            .any(|t| t.get("tool").and_then(|v| v.as_str()) == Some("system")),
        "tools.list should include system; got: {tools:?}"
    );
}

#[test]
fn system_schema_list_filters_by_portal_and_includes_tasks_snapshot() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_list_filters_by_portal_and_includes_tasks_snapshot",
        &["--workspace", "ws_schema_list"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.list",
                "args": { "portal": "tasks", "q": "search", "limit": 50 }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system schema.list must succeed; got: {text}"
    );
    let schemas = text
        .get("result")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_array())
        .expect("result.schemas");
    assert!(
        schemas
            .iter()
            .any(|v| v.get("cmd").and_then(|v| v.as_str()) == Some("tasks.search")),
        "schema.list should include tasks.search; got schemas={schemas:?}"
    );
}

#[test]
fn system_tutorial_is_callable_as_op_without_call_cmd() {
    let mut server = Server::start_initialized_with_args(
        "system_tutorial_is_callable_as_op_without_call_cmd",
        &["--workspace", "ws_tutorial_op"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "tutorial", "args": { "limit": 2 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system tutorial must succeed; got: {text}"
    );
    let steps = text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("result.steps");
    assert!(
        steps.len() <= 2,
        "tutorial limit=2 should cap steps; got steps={steps:?}"
    );
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !actions.is_empty(),
        "tutorial should include actions[]; got: {text}"
    );
}

#[test]
fn system_quickstart_tasks_returns_3_to_5_recipes_and_actions() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_tasks_returns_3_to_5_recipes_and_actions",
        &["--workspace", "ws_quickstart_tasks"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": "tasks" } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system quickstart must succeed; got: {text}"
    );

    let result = text.get("result").expect("result");
    assert_eq!(
        result.get("portal").and_then(|v| v.as_str()),
        Some("tasks"),
        "quickstart portal should be tasks; got: {text}"
    );
    assert_eq!(
        result.get("workspace_selected").and_then(|v| v.as_str()),
        Some("ws_quickstart_tasks"),
        "quickstart should expose currently selected workspace; got: {text}"
    );
    assert_eq!(
        result
            .get("workspace_selected_source")
            .and_then(|v| v.as_str()),
        Some("default_workspace"),
        "quickstart should expose why this workspace is selected; got: {text}"
    );
    assert!(
        result.get("defaults").is_some(),
        "quickstart should include defaults block; got: {text}"
    );

    let recipes = result
        .get("recipes")
        .and_then(|v| v.as_array())
        .expect("result.recipes");
    assert!(
        recipes.len() >= 3 && recipes.len() <= 5,
        "quickstart should return 3–5 recipes; got recipes={recipes:?}"
    );
    assert!(
        recipes.iter().all(|r| r.get("uses_defaults").is_some()),
        "quickstart recipes should include uses_defaults field; got recipes={recipes:?}"
    );

    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("tasks")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("tasks.snapshot")
        }),
        "quickstart should include a tasks.snapshot action; got actions={actions:?}"
    );
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("tasks")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("tasks.exec.summary")
        }),
        "quickstart should include a one-command preset tasks.exec.summary; got actions={actions:?}"
    );
}

#[test]
fn tasks_exec_summary_preset_returns_exec_and_critical_regressions_blocks() {
    let mut server = Server::start_initialized_with_args(
        "tasks_exec_summary_preset_returns_exec_and_critical_regressions_blocks",
        &["--workspace", "ws_exec_summary_preset"],
    );

    let create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_exec_summary_preset",
                "op": "call",
                "cmd": "tasks.macro.start",
                "args": { "task_title": "Preset smoke task", "template": "basic-task" }
            }
        }
    }));
    assert!(
        !tool_call_is_error(&create),
        "tasks.macro.start should complete in preset smoke test; raw={create}"
    );

    let preset = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_exec_summary_preset",
                "op": "call",
                "cmd": "tasks.exec.summary",
                "args": {}
            }
        }
    }));
    let preset_text = extract_tool_text(&preset);
    assert_eq!(
        preset_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tasks.exec.summary preset should succeed; got: {preset_text}"
    );
    let result = preset_text.get("result").expect("result");
    assert!(
        result.get("exec_summary").is_some(),
        "tasks.exec.summary should include exec_summary; got: {preset_text}"
    );
    assert!(
        result
            .get("critical_regressions")
            .and_then(|v| v.as_array())
            .is_some(),
        "tasks.exec.summary should include critical_regressions array; got: {preset_text}"
    );
    assert_eq!(
        result
            .get("source")
            .and_then(|v| v.get("exec_summary"))
            .and_then(|v| v.as_str()),
        Some("tasks.handoff"),
        "tasks.exec.summary should disclose source.exec_summary=tasks.handoff; got: {preset_text}"
    );
    assert_eq!(
        result
            .get("source")
            .and_then(|v| v.get("regressions"))
            .and_then(|v| v.as_str()),
        Some("tasks.lint"),
        "tasks.exec.summary should disclose source.regressions=tasks.lint; got: {preset_text}"
    );
}

#[test]
fn system_exec_summary_returns_cross_portal_pulse() {
    let mut server = Server::start_initialized_with_args(
        "system_exec_summary_returns_cross_portal_pulse",
        &["--workspace", "ws_system_exec_summary"],
    );

    let create = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_system_exec_summary",
                "op": "call",
                "cmd": "tasks.macro.start",
                "args": { "task_title": "System exec summary smoke", "template": "basic-task" }
            }
        }
    }));
    assert!(
        !tool_call_is_error(&create),
        "tasks.macro.start should complete in system exec summary smoke; raw={create}"
    );

    let summary = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "workspace": "ws_system_exec_summary",
                "op": "call",
                "cmd": "system.exec.summary",
                "args": {}
            }
        }
    }));
    let summary_text = extract_tool_text(&summary);
    assert_eq!(
        summary_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system.exec.summary should succeed; got: {summary_text}"
    );
    let result = summary_text.get("result").expect("result");
    assert!(
        result.get("summary").and_then(|v| v.get("tasks")).is_some(),
        "system.exec.summary should include summary.tasks; got: {summary_text}"
    );
    assert!(
        result.get("summary").and_then(|v| v.get("jobs")).is_some(),
        "system.exec.summary should include summary.jobs; got: {summary_text}"
    );
    assert!(
        result
            .get("critical_regressions")
            .and_then(|v| v.as_array())
            .is_some(),
        "system.exec.summary should include critical_regressions[]; got: {summary_text}"
    );
    assert_eq!(
        result
            .get("source")
            .and_then(|v| v.get("tasks"))
            .and_then(|v| v.as_str()),
        Some("tasks.exec.summary"),
        "system.exec.summary should disclose source.tasks; got: {summary_text}"
    );
    assert_eq!(
        result
            .get("source")
            .and_then(|v| v.get("jobs"))
            .and_then(|v| v.as_str()),
        Some("jobs.control.center"),
        "system.exec.summary should disclose source.jobs; got: {summary_text}"
    );
}

#[test]
fn schema_get_jobs_report_exposes_progress_checkpoint_contract() {
    let mut server = Server::start_initialized_with_args(
        "schema_get_jobs_report_exposes_progress_checkpoint_contract",
        &["--workspace", "ws_schema_jobs_report_contract"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "workspace": "ws_schema_jobs_report_contract",
                "op": "schema.get",
                "args": { "cmd": "jobs.report" }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system schema.get jobs.report should succeed; got: {text}"
    );

    let schema = text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .expect("result.args_schema");
    let minimal = text
        .get("result")
        .and_then(|v| v.get("example_minimal_args"))
        .expect("result.example_minimal_args");
    assert!(
        minimal.is_object(),
        "example_minimal_args must remain an object (not placeholder) after conditional schema guards; got: {minimal}"
    );
    assert!(
        minimal.get("job").is_some()
            && minimal.get("runner_id").is_some()
            && minimal.get("claim_revision").is_some(),
        "example_minimal_args should still expose base required keys for jobs.report; got: {minimal}"
    );
    let all_of = schema.get("allOf").and_then(|v| v.as_array()).expect(
        "jobs.report args_schema.allOf must exist for conditional progress/checkpoint contract",
    );
    let conditional = all_of
        .iter()
        .find(|rule| {
            rule.get("if")
                .and_then(|v| v.get("properties"))
                .and_then(|v| v.get("kind"))
                .and_then(|v| v.get("enum"))
                .and_then(|v| v.as_array())
                .is_some_and(|kinds| {
                    kinds.iter().any(|k| k.as_str() == Some("progress"))
                        && kinds.iter().any(|k| k.as_str() == Some("checkpoint"))
                })
        })
        .expect("conditional rule for kind=progress|checkpoint must be present");
    let then_step = conditional
        .get("then")
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("meta"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("step"))
        .expect("then.properties.meta.properties.step must exist");
    assert!(
        then_step
            .get("required")
            .and_then(|v| v.as_array())
            .is_some_and(|required| required.iter().any(|v| v.as_str() == Some("command"))),
        "step.required must include command; got step={then_step}"
    );
    assert!(
        then_step
            .get("anyOf")
            .and_then(|v| v.as_array())
            .is_some_and(|rules| rules.len() >= 2),
        "step.anyOf must require result or error; got step={then_step}"
    );
}

#[test]
fn system_quickstart_system_includes_exec_summary_recipe() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_system_includes_exec_summary_recipe",
        &["--workspace", "ws_quickstart_system_exec_summary"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": "system" } } }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system quickstart portal=system must succeed; got: {text}"
    );
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("system")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("exec.summary")
        }),
        "system quickstart should include system op=exec.summary recipe; got actions={actions:?}"
    );
}

#[test]
fn jobs_exec_summary_returns_meaning_first_minimal_blocks() {
    let mut server = Server::start_initialized_with_args(
        "jobs_exec_summary_returns_meaning_first_minimal_blocks",
        &["--workspace", "ws_jobs_exec_summary"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_exec_summary",
                "op": "exec.summary",
                "args": {}
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.exec.summary should succeed; got: {text}"
    );
    let result = text.get("result").expect("result");
    assert!(
        result
            .get("now")
            .and_then(|v| v.get("headline"))
            .and_then(|v| v.as_str())
            .is_some(),
        "jobs.exec.summary should expose now.headline; got: {text}"
    );
    assert!(
        result
            .get("proven")
            .and_then(|v| v.get("guardrails"))
            .is_some(),
        "jobs.exec.summary should expose proven.guardrails; got: {text}"
    );
    assert_eq!(
        result
            .get("proven")
            .and_then(|v| v.get("guardrails"))
            .and_then(|v| v.get("jobs_wait_timeout_cap_ms"))
            .and_then(|v| v.as_i64()),
        Some(25_000),
        "jobs.exec.summary must report the transport-safe jobs.wait cap (25000ms); got: {text}"
    );
    assert!(
        result
            .get("critical_regressions")
            .and_then(|v| v.as_array())
            .is_some(),
        "jobs.exec.summary should expose critical_regressions[]; got: {text}"
    );
    assert!(
        result
            .get("next")
            .and_then(|v| v.as_array())
            .is_some_and(|items| !items.is_empty()),
        "jobs.exec.summary should expose non-empty next[] from control-center action-pack; got: {text}"
    );
    assert_eq!(
        result.get("source").and_then(|v| v.as_str()),
        Some("jobs.control.center"),
        "jobs.exec.summary should disclose source=jobs.control.center; got: {text}"
    );
    assert!(
        result.get("details").is_none(),
        "jobs.exec.summary should stay minimal by default (no details block); got: {text}"
    );
}

#[test]
fn jobs_exec_summary_rejects_unknown_args_fail_closed() {
    let mut server = Server::start_initialized_with_args(
        "jobs_exec_summary_rejects_unknown_args_fail_closed",
        &["--workspace", "ws_jobs_exec_summary_strict_args"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_exec_summary_strict_args",
                "op": "exec.summary",
                "args": { "unknown_probe": 1 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs.exec.summary must fail-closed on unknown args; got: {text}"
    );
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT"),
        "jobs.exec.summary unknown args should return INVALID_INPUT; got: {text}"
    );
}

#[test]
fn system_quickstart_jobs_includes_exec_summary_recipe() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_jobs_includes_exec_summary_recipe",
        &["--workspace", "ws_quickstart_jobs_exec_summary"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": "jobs" } } }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system quickstart portal=jobs must succeed; got: {text}"
    );
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("jobs")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("exec.summary")
        }),
        "system quickstart portal=jobs should include jobs op=exec.summary; got actions={actions:?}"
    );
}

#[test]
fn system_quickstart_other_curated_portals_are_dispatchable() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_other_curated_portals_are_dispatchable",
        &["--workspace", "ws_quickstart_more"],
    );

    let portals = ["status", "open", "think", "graph", "vcs", "docs"];
    for (idx, portal) in portals.iter().enumerate() {
        let resp = server.request(json!({
            "jsonrpc": "2.0",
            "id": 100 + idx,
            "method": "tools/call",
            "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": portal } } }
        }));

        let text = extract_tool_text(&resp);
        assert_eq!(
            text.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "system quickstart portal={portal} must succeed; got: {text}"
        );

        let result = text.get("result").expect("result");
        assert_eq!(
            result.get("portal").and_then(|v| v.as_str()),
            Some(*portal),
            "quickstart portal should roundtrip; got: {text}"
        );
        assert!(
            result.get("defaults").is_some(),
            "quickstart portal={portal} should include defaults block; got: {text}"
        );
        let recipes = result
            .get("recipes")
            .and_then(|v| v.as_array())
            .expect("result.recipes");
        assert!(
            recipes.len() >= 3 && recipes.len() <= 5,
            "quickstart portal={portal} should return 3–5 recipes; got recipes={recipes:?}"
        );
        assert!(
            recipes.iter().all(|r| r.get("uses_defaults").is_some()),
            "quickstart portal={portal} recipes should include uses_defaults; got recipes={recipes:?}"
        );

        let actions = text
            .get("actions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            !actions.is_empty(),
            "quickstart portal={portal} should include actions[]; got text={text}"
        );

        let expected_action_ok = match *portal {
            "status" => actions
                .iter()
                .any(|a| a.get("tool").and_then(|v| v.as_str()) == Some("status")),
            "open" => actions.iter().any(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("open")
                    && a.get("args")
                        .and_then(|v| v.get("id"))
                        .and_then(|v| v.as_str())
                        == Some("a:core")
            }),
            "think" => actions.iter().any(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("think")
                    && a.get("args")
                        .and_then(|v| v.get("op"))
                        .and_then(|v| v.as_str())
                        == Some("reasoning.seed")
            }),
            "graph" => actions.iter().any(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("graph")
                    && a.get("args")
                        .and_then(|v| v.get("op"))
                        .and_then(|v| v.as_str())
                        == Some("query")
            }),
            "vcs" => actions.iter().any(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("vcs")
                    && a.get("args")
                        .and_then(|v| v.get("cmd"))
                        .and_then(|v| v.as_str())
                        == Some("vcs.branch.list")
            }),
            "docs" => actions.iter().any(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("docs")
                    && a.get("args")
                        .and_then(|v| v.get("op"))
                        .and_then(|v| v.as_str())
                        == Some("list")
            }),
            _ => false,
        };
        assert!(
            expected_action_ok,
            "quickstart portal={portal} should include an expected action; got actions={actions:?}"
        );
    }
}

#[test]
fn system_quickstart_think_includes_sequential_checkpoint_recipe() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_think_includes_sequential_checkpoint_recipe",
        &["--workspace", "ws_quickstart_think_seq"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": "think", "limit": 5 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system quickstart portal=think must succeed; got: {text}"
    );

    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("think")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("think.trace.sequential.step")
        }),
        "quickstart portal=think should include sequential checkpoint recipe; got actions={actions:?}"
    );
}

#[test]
fn system_quickstart_actions_execute_as_is_under_declared_budget_profile() {
    let mut server = Server::start_initialized_with_args(
        "system_quickstart_actions_execute_as_is_under_declared_budget_profile",
        &["--workspace", "ws_quickstart_e2e"],
    );

    let portals = [
        "status",
        "open",
        "tasks",
        "jobs",
        "workspace",
        "think",
        "graph",
        "vcs",
        "docs",
        "system",
    ];

    let mut id: i64 = 10_000;
    for portal in portals {
        id += 1;
        let quick = server.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": "system", "arguments": { "op": "quickstart", "args": { "portal": portal, "limit": 5 } } }
        }));

        let quick_text = extract_tool_text(&quick);
        assert_eq!(
            quick_text.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "system quickstart portal={portal} must succeed; got: {quick_text}"
        );

        let actions = quick_text
            .get("actions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            !actions.is_empty(),
            "quickstart portal={portal} must return executable actions[]; got: {quick_text}"
        );

        for action in actions {
            let action_id = action
                .get("action_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown-action");
            let tool = action
                .get("tool")
                .and_then(|v| v.as_str())
                .expect("action.tool");
            let args = action.get("args").cloned().expect("action.args");

            let budget_profile = args
                .get("budget_profile")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert!(
                !budget_profile.is_empty(),
                "quickstart action {action_id} must carry budget_profile for copy/paste reliability; action={action:?}"
            );

            id += 1;
            let resp = server.request(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": { "name": tool, "arguments": args }
            }));

            let resp_text = extract_tool_text_str(&resp);
            assert!(
                !tool_call_is_error(&resp),
                "quickstart action failed portal={portal} action_id={action_id} tool={tool} budget_profile={budget_profile} response={resp_text}"
            );
        }
    }
}

#[test]
fn workspace_list_exposes_explicit_selection_markers() {
    let mut server = Server::start_initialized_with_args(
        "workspace_list_exposes_explicit_selection_markers",
        &["--workspace", "ws_selection_markers"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "workspace", "arguments": { "op": "list", "args": { "limit": 20 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "workspace list must succeed; got: {text}"
    );
    let result = text.get("result").expect("result");
    assert_eq!(
        result.get("selected_workspace").and_then(|v| v.as_str()),
        Some("ws_selection_markers"),
        "selected_workspace should show current active workspace; got: {result}"
    );
    assert_eq!(
        result.get("active_workspace").and_then(|v| v.as_str()),
        Some("ws_selection_markers"),
        "active_workspace marker must mirror current selection; got: {result}"
    );
    assert_eq!(
        result
            .get("selected_workspace_source")
            .and_then(|v| v.as_str()),
        Some("default_workspace"),
        "selected_workspace_source should explain why this workspace is active; got: {result}"
    );
    assert_eq!(
        result.get("requested_workspace").and_then(|v| v.as_str()),
        Some("ws_selection_markers"),
        "requested_workspace should show which workspace was applied to this call; got: {result}"
    );
}

#[test]
fn system_schema_get_accepts_top_level_cmd_shorthand() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_get_accepts_top_level_cmd_shorthand",
        &["--workspace", "ws_schema_shorthand"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "schema.get", "cmd": "tasks.snapshot" } }
    }));
    let text = extract_tool_text(&resp);

    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get shorthand (top-level cmd) should succeed; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("tasks.snapshot"),
        "schema.get shorthand should resolve cmd=tasks.snapshot; got: {text}"
    );
}

#[test]
fn schema_get_with_portal_returns_schema_list_action() {
    let mut server = Server::start_initialized_with_args(
        "schema_get_with_portal_returns_schema_list_action",
        &["--workspace", "ws_schema_get_portal"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "schema.get", "args": { "portal": "tasks" } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "schema.get without cmd should error; got: {text}"
    );
    let code = text
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(code, Some("INVALID_INPUT"));
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("system")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("schema.list")
        }),
        "schema.get portal misuse should suggest schema.list; got actions={actions:?}"
    );
}

#[test]
fn system_schema_list_mode_golden_defaults_to_summary_rows() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_list_mode_golden_defaults_to_summary_rows",
        &["--workspace", "ws_schema_list_golden_mode"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.list",
                "args": { "portal": "jobs", "q": "summary", "limit": 50 }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system schema.list must succeed; got: {text}"
    );
    let schemas = text
        .get("result")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_array())
        .expect("result.schemas");
    let exec_summary = schemas
        .iter()
        .find(|s| s.get("cmd").and_then(|v| v.as_str()) == Some("jobs.exec.summary"))
        .expect("jobs.exec.summary in schema.list");
    assert!(
        exec_summary.get("required").is_none(),
        "golden/default mode should not include detailed required hints; got={exec_summary:?}"
    );
    assert!(
        exec_summary.get("required_any_of").is_none(),
        "golden/default mode should not include required_any_of; got={exec_summary:?}"
    );
}

#[test]
fn system_schema_list_mode_all_includes_required_fields() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_list_mode_all_includes_required_fields",
        &["--workspace", "ws_schema_list_all_mode"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.list",
                "args": { "portal": "jobs", "mode": "all", "q": "summary", "limit": 50 }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system schema.list all mode must succeed; got: {text}"
    );
    let schemas = text
        .get("result")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_array())
        .expect("result.schemas");
    let exec_summary = schemas
        .iter()
        .find(|s| s.get("cmd").and_then(|v| v.as_str()) == Some("jobs.exec.summary"))
        .expect("jobs.exec.summary in schema.list");
    assert!(
        exec_summary.get("required").is_some(),
        "all mode should include required in schema list row; got={exec_summary:?}"
    );
    assert!(
        exec_summary.get("required_any_of").is_some(),
        "all mode should include required_any_of in schema list row; got={exec_summary:?}"
    );
}

#[test]
fn system_cmd_list_supports_q_filter() {
    let mut server = Server::start_initialized_with_args(
        "system_cmd_list_supports_q_filter",
        &["--workspace", "ws_cmd_list_q"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "cmd.list", "args": { "q": "schema", "limit": 200 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system cmd.list must succeed; got: {text}"
    );
    let cmds = text
        .get("result")
        .and_then(|v| v.get("cmds"))
        .and_then(|v| v.as_array())
        .expect("result.cmds");
    assert!(
        cmds.iter().any(|v| v.as_str() == Some("system.schema.get")),
        "q filter should include system.schema.get; got cmds={cmds:?}"
    );
}

#[test]
fn system_cmd_list_default_mode_is_golden() {
    let mut server = Server::start_initialized_with_args(
        "system_cmd_list_default_mode_is_golden",
        &["--workspace", "ws_cmd_list_golden_mode"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "cmd.list", "args": { "q": "system.", "limit": 200 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system cmd.list must succeed; got: {text}"
    );
    let cmds = text
        .get("result")
        .and_then(|v| v.get("cmds"))
        .and_then(|v| v.as_array())
        .expect("result.cmds");

    assert!(
        cmds.iter()
            .any(|v| v.as_str() == Some("system.schema.list")),
        "default cmd.list(golden) should include system.schema.list; got cmds={cmds:?}"
    );
    assert!(
        !cmds.iter().any(|v| v.as_str() == Some("system.cmd.list")),
        "default cmd.list should not include advanced cmd list; got cmds={cmds:?}"
    );
}

#[test]
fn system_cmd_list_all_mode_includes_advanced_ops() {
    let mut server = Server::start_initialized_with_args(
        "system_cmd_list_all_mode_includes_advanced_ops",
        &["--workspace", "ws_cmd_list_all_mode"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "cmd.list", "args": { "q": "system.", "mode": "all", "limit": 200 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system cmd.list all mode must succeed; got: {text}"
    );
    let cmds = text
        .get("result")
        .and_then(|v| v.get("cmds"))
        .and_then(|v| v.as_array())
        .expect("result.cmds");

    assert!(
        cmds.iter().any(|v| v.as_str() == Some("system.cmd.list")),
        "all mode should include system.cmd.list; got cmds={cmds:?}"
    );
}

#[test]
fn system_cmd_list_unknown_arg_returns_unknown_arg() {
    let mut server = Server::start_initialized_with_args(
        "system_cmd_list_unknown_arg_returns_unknown_arg",
        &["--workspace", "ws_cmd_list_unknown_arg"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "cmd.list", "args": { "limit": 20, "nope": true } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "system cmd.list unknown arg must fail; got: {text}"
    );
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ARG"),
        "system cmd.list unknown arg must return UNKNOWN_ARG; got: {text}"
    );
}

#[test]
fn system_schema_get_exposes_cmd_list_q_and_jobs_open_include_artifacts() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_get_exposes_cmd_list_q_and_jobs_open_include_artifacts",
        &["--workspace", "ws_schema_fields"],
    );

    let cmd_list_schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "schema.get", "args": { "cmd": "system.cmd.list" } } }
    }));
    let cmd_list_text = extract_tool_text(&cmd_list_schema);
    assert_eq!(
        cmd_list_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system.schema.get(system.cmd.list) must succeed; got: {cmd_list_text}"
    );
    let cmd_list_has_q = cmd_list_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("q"))
        .is_some();
    assert!(cmd_list_has_q, "system.cmd.list args_schema must include q");

    let jobs_open_schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "schema.get", "args": { "cmd": "jobs.open" } } }
    }));
    let jobs_open_text = extract_tool_text(&jobs_open_schema);
    assert_eq!(
        jobs_open_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system.schema.get(jobs.open) must succeed; got: {jobs_open_text}"
    );
    let jobs_open_has_include_artifacts = jobs_open_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("include_artifacts"))
        .is_some();
    assert!(
        jobs_open_has_include_artifacts,
        "jobs.open args_schema must include include_artifacts"
    );
}

#[test]
fn status_workspace_path_resolves_and_succeeds() {
    // Do not configure a default workspace here: we want to verify that an explicit path
    // is resolved to a stable WorkspaceId and the call succeeds.
    let mut server =
        Server::start_initialized_with_args("status_workspace_path_resolves_and_succeeds", &[]);

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": "/tmp/my_repo" } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "status call with workspace path should succeed; got: {text}"
    );
    assert!(
        text.get("result")
            .and_then(|v| v.get("workspace"))
            .and_then(|v| v.as_str())
            == Some("my-repo"),
        "resolved workspace should be a basename slug (my-repo); got: {text}"
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "workspace", "arguments": { "op": "list", "args": { "limit": 200 } } }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "workspace op=list must succeed; got: {text}"
    );
    let workspaces = text
        .get("result")
        .and_then(|v| v.get("workspaces"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        workspaces.iter().any(|w| {
            w.get("workspace").and_then(|v| v.as_str()) == Some("my-repo")
                && w.get("bound_path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|p| p.ends_with("/my_repo") || p.ends_with("\\my_repo"))
        }),
        "workspace list should include a bound_path for the /tmp/my_repo mapping; got workspaces={workspaces:?}"
    );
}

#[test]
fn open_by_path_jumps_to_bound_anchor() {
    let mut server = Server::start_initialized("open_by_path_jumps_to_bound_anchor");

    // 1) Create an anchor bound to a repo-relative path.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws1",
                "op": "call",
                "cmd": "think.macro.anchor.note",
                "args": {
                    "anchor": "a:core",
                    "title": "Core",
                    "kind": "component",
                    "bind_paths": ["crates/mcp/src"],
                    "content": "Bind core area.",
                    "card_type": "note",
                    "visibility": "canon"
                }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "anchor bind via macro_anchor_note must succeed; got: {text}"
    );

    // 2) Resolve a deeper path → should match the bound prefix and return a:core.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws1",
                "op": "call",
                "cmd": "think.anchor.resolve",
                "args": { "path": "crates/mcp/src/handlers/branchmind/core/open/mod.rs", "limit": 10 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think.anchor.resolve must succeed; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("best"))
            .and_then(|v| v.get("anchor"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some("a:core"),
        "best anchor should be a:core; got: {text}"
    );

    // 3) Open by path should auto-jump to the anchor and include a jump block.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "open",
            "arguments": {
                "workspace": "ws1",
                "id": "crates/mcp/src/handlers/branchmind/core/open/mod.rs",
                "limit": 5
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "open by path must succeed; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("anchor"),
        "open(kind) must be anchor; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("anchor"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str()),
        Some("a:core"),
        "open(anchor.id) must be a:core; got: {text}"
    );
    assert!(
        text.get("result").and_then(|v| v.get("jump")).is_some(),
        "open by path must include jump block; got: {text}"
    );
}

#[test]
fn atlas_suggest_apply_and_list_bindings_work() {
    let mut server = Server::start_initialized("atlas_suggest_apply_and_list_bindings_work");

    // Create a tiny repo-like directory structure.
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let repo_root = std::env::temp_dir().join(format!("bm_atlas_repo_{pid}_{nonce}"));
    std::fs::create_dir_all(repo_root.join("docs")).expect("mkdir docs");
    std::fs::create_dir_all(repo_root.join("infra")).expect("mkdir infra");
    std::fs::create_dir_all(repo_root.join("crates/mcp/src")).expect("mkdir crates/mcp/src");
    std::fs::create_dir_all(repo_root.join("crates/storage/src"))
        .expect("mkdir crates/storage/src");

    // Bind workspace to repo_root by using status with a path workspace.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": repo_root.to_string_lossy() } }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "status must succeed for repo_root binding; got: {text}"
    );
    let ws_id = text
        .get("result")
        .and_then(|v| v.get("workspace"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !ws_id.is_empty(),
        "status must return a resolved workspace id; got: {text}"
    );

    // Suggest an atlas for the repo.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": ws_id,
                "op": "call",
                "cmd": "think.atlas.suggest",
                "args": { "granularity": "depth2", "limit": 20 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think.atlas.suggest must succeed; got: {text}"
    );

    // Execute the returned apply action.
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let apply = actions.iter().find(|a| {
        a.get("tool").and_then(|v| v.as_str()) == Some("think")
            && a.get("args")
                .and_then(|v| v.get("cmd"))
                .and_then(|v| v.as_str())
                == Some("think.macro.atlas.apply")
    });
    let apply = apply.expect("atlas suggest should include an apply action");
    let tool = apply.get("tool").and_then(|v| v.as_str()).expect("tool");
    let args = apply.get("args").cloned().expect("args");

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think.macro.atlas.apply must succeed; got: {text}"
    );

    // Listing bindings should show the crates container children.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": text.get("result").and_then(|v| v.get("workspace")).and_then(|v| v.as_str()).unwrap_or(""),
                "op": "call",
                "cmd": "think.atlas.bindings.list",
                "args": { "prefix": "crates", "limit": 200 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "think.atlas.bindings.list must succeed; got: {text}"
    );
    let bindings = text
        .get("result")
        .and_then(|v| v.get("bindings"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        bindings
            .iter()
            .any(|b| b.get("repo_rel").and_then(|v| v.as_str()) == Some("crates/mcp")),
        "bindings should include crates/mcp; got bindings={bindings:?}"
    );

    // Open by a deeper path should jump to the bound anchor.
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "open",
            "arguments": {
                "workspace": text.get("result").and_then(|v| v.get("workspace")).and_then(|v| v.as_str()).unwrap_or(""),
                "id": "crates/mcp/src",
                "limit": 5
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "open by path must succeed after atlas apply; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("anchor"),
        "open(kind) must be anchor; got: {text}"
    );
    assert!(
        text.get("result").and_then(|v| v.get("jump")).is_some(),
        "open by path must include jump block; got: {text}"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
}
