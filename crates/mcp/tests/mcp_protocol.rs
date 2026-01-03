#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn mcp_requires_notifications_initialized() {
    let mut server = Server::start("requires_initialized");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert!(
        init.get("result").is_some(),
        "initialize must return result"
    );

    let tools_list_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    assert_json_rpc_error(&tools_list_before, -32002);

    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let mut names = tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "branch_create",
            "branch_delete",
            "branch_list",
            "branch_rename",
            "checkout",
            "commit",
            "context_pack",
            "diagnostics",
            "diff",
            "docs_list",
            "export",
            "graph_apply",
            "graph_conflict_resolve",
            "graph_conflict_show",
            "graph_conflicts",
            "graph_diff",
            "graph_merge",
            "graph_query",
            "graph_validate",
            "help",
            "init",
            "log",
            "macro_branch_note",
            "merge",
            "notes_commit",
            "reflog",
            "reset",
            "show",
            "status",
            "storage",
            "tag_create",
            "tag_delete",
            "tag_list",
            "tasks_batch",
            "tasks_block",
            "tasks_bootstrap",
            "tasks_close_step",
            "tasks_complete",
            "tasks_context",
            "tasks_context_pack",
            "tasks_contract",
            "tasks_create",
            "tasks_decompose",
            "tasks_define",
            "tasks_delete",
            "tasks_delta",
            "tasks_done",
            "tasks_edit",
            "tasks_evidence_capture",
            "tasks_focus_clear",
            "tasks_focus_get",
            "tasks_focus_set",
            "tasks_handoff",
            "tasks_history",
            "tasks_lint",
            "tasks_macro_close_step",
            "tasks_macro_create_done",
            "tasks_macro_finish",
            "tasks_macro_start",
            "tasks_mirror",
            "tasks_note",
            "tasks_patch",
            "tasks_plan",
            "tasks_progress",
            "tasks_radar",
            "tasks_redo",
            "tasks_resume",
            "tasks_resume_pack",
            "tasks_resume_super",
            "tasks_scaffold",
            "tasks_snapshot",
            "tasks_storage",
            "tasks_task_add",
            "tasks_task_define",
            "tasks_task_delete",
            "tasks_templates_list",
            "tasks_undo",
            "tasks_verify",
            "think_add_decision",
            "think_add_evidence",
            "think_add_frame",
            "think_add_hypothesis",
            "think_add_note",
            "think_add_question",
            "think_add_test",
            "think_add_update",
            "think_card",
            "think_context",
            "think_frontier",
            "think_link",
            "think_lint",
            "think_next",
            "think_nominal_merge",
            "think_pack",
            "think_pin",
            "think_pins",
            "think_pipeline",
            "think_playbook",
            "think_query",
            "think_set_status",
            "think_subgoal_close",
            "think_subgoal_open",
            "think_template",
            "think_watch",
            "trace_hydrate",
            "trace_sequential_step",
            "trace_step",
            "trace_validate",
        ]
    );
}
#[test]
fn tools_schema_has_steps_items() {
    let mut server = Server::start("tools_schema_steps_items");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let tasks_create = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_create"))
        .expect("tasks_create tool");
    let steps_items = tasks_create
        .get("inputSchema")
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.get("items"));
    assert!(
        steps_items.is_some(),
        "tasks_create inputSchema.steps must declare items"
    );
}

#[test]
fn tools_schema_focus_set_does_not_require_task() {
    let mut server = Server::start("tools_schema_focus_set_optional_task");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let focus_set = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_focus_set"))
        .expect("tasks_focus_set tool");
    let required = focus_set
        .get("inputSchema")
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("tasks_focus_set inputSchema.required");
    assert_eq!(
        required
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["workspace"]
    );
}

#[test]
fn tools_schema_macro_start_supports_template() {
    let mut server = Server::start("tools_schema_macro_start_supports_template");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let macro_start = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_macro_start"))
        .expect("tasks_macro_start tool");

    let properties = macro_start
        .get("inputSchema")
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .expect("tasks_macro_start inputSchema.properties");
    assert!(
        properties.contains_key("template"),
        "tasks_macro_start must declare template"
    );
    assert!(
        properties.contains_key("think"),
        "tasks_macro_start must declare think passthrough"
    );

    let required = macro_start
        .get("inputSchema")
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("tasks_macro_start inputSchema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("steps")),
        "tasks_macro_start must not require steps (template is allowed)"
    );
}

#[test]
fn tools_schema_macro_close_step_does_not_require_task() {
    let mut server = Server::start("tools_schema_macro_close_step_optional_task");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let macro_close = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_macro_close_step"))
        .expect("tasks_macro_close_step tool");
    let required = macro_close
        .get("inputSchema")
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("tasks_macro_close_step inputSchema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("task")),
        "tasks_macro_close_step must not require task (focus-first)"
    );
}

#[test]
fn tools_schema_portal_tools_do_not_require_workspace() {
    let mut server = Server::start("tools_schema_portal_tools_do_not_require_workspace");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    for name in [
        "status",
        "macro_branch_note",
        "tasks_macro_start",
        "tasks_macro_close_step",
        "tasks_snapshot",
    ] {
        let tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
            .unwrap_or_else(|| panic!("{name} tool"));
        let required = tool
            .get("inputSchema")
            .and_then(|v| v.get("required"))
            .and_then(|v| v.as_array())
            .expect("inputSchema.required");
        assert!(
            !required.iter().any(|v| v.as_str() == Some("workspace")),
            "{name} must not require workspace (portal DX)"
        );
    }
}

#[test]
fn tools_list_daily_toolset_is_curated() {
    let mut server = Server::start_initialized_with_args(
        "tools_list_daily_toolset_is_curated",
        &["--toolset", "daily"],
    );

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let has_snapshot = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_snapshot"));
    assert!(
        has_snapshot,
        "daily toolset must include tasks_snapshot (handoff/resume portal)"
    );

    let has_branch_note = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("macro_branch_note"));
    assert!(
        has_branch_note,
        "daily toolset must include macro_branch_note (branching portal)"
    );

    let has_close_step = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_macro_close_step"));
    assert!(
        has_close_step,
        "daily toolset must include tasks_macro_close_step (progress portal)"
    );

    let has_tag_delete = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tag_delete"));
    assert!(!has_tag_delete, "daily toolset must hide tag_delete");

    assert!(
        tools.len() <= 5,
        "daily toolset must stay extremely small (<= 5 tools)"
    );
}

#[test]
fn tools_list_params_can_override_toolset() {
    let mut server = Server::start_initialized_with_args(
        "tools_list_params_can_override_toolset",
        &["--toolset", "daily"],
    );

    let daily_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let daily_tools = daily_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("daily result.tools");
    assert!(
        daily_tools.len() <= 5,
        "server daily toolset should advertise <= 5 tools"
    );

    let full_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/list",
        "params": { "toolset": "full" }
    }));
    let full_tools = full_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("full result.tools");
    assert!(
        full_tools.len() > daily_tools.len(),
        "full override should reveal more tools than daily"
    );

    let has_edit = full_tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_edit"));
    assert!(has_edit, "full override should include tasks_edit");
}

#[test]
fn tools_list_core_toolset_is_minimal() {
    let mut server = Server::start_initialized_with_args(
        "tools_list_core_toolset_is_minimal",
        &["--toolset", "core"],
    );

    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let has_snapshot = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_snapshot"));
    assert!(has_snapshot, "core toolset must include tasks_snapshot");

    let has_edit = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_edit"));
    assert!(!has_edit, "core toolset must hide tasks_edit");

    let has_branch_note = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("macro_branch_note"));
    assert!(
        !has_branch_note,
        "core toolset must hide macro_branch_note (use daily for branching)"
    );

    let has_close_step = tools
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("tasks_macro_close_step"));
    assert!(
        !has_close_step,
        "core toolset must hide tasks_macro_close_step (use daily for progress ops)"
    );

    assert!(
        tools.len() <= 3,
        "core toolset must be ultra-minimal (<= 3 tools)"
    );
}
#[test]
fn auto_init_workspace_and_target_ref_aliases() {
    let mut server = Server::start("auto_init_target_ref");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    // Portals are context-first (BM-L1 lines), so for structured verification we use explicit view tools.
    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws_auto" } }
    }));
    let context_text = extract_tool_text(&context);
    assert_eq!(
        context_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "workspace should auto-init on first call"
    );
    assert_eq!(
        context_text
            .get("result")
            .and_then(|v| v.get("workspace"))
            .and_then(|v| v.as_str()),
        Some("ws_auto")
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_auto", "kind": "plan", "title": "Plan Auto" } }
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
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_auto", "kind": "task", "parent": plan_id.clone(), "title": "Task Auto", "steps": [ { "title": "Step 1", "success_criteria": ["ok"] } ] } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step_id = created_task_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    let focus_set = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws_auto", "target": { "id": plan_id, "kind": "plan" } } }
    }));
    let focus_set_text = extract_tool_text(&focus_set);
    assert_eq!(
        focus_set_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_auto", "target": { "id": task_id } } }
    }));
    let radar_text = extract_tool_text(&radar);
    assert_eq!(
        radar_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws_auto", "target": { "id": task_id, "kind": "task" }, "content": "auto-init ok" } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let edit_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks_edit", "arguments": { "workspace": "ws_auto", "target": { "id": plan_id.clone(), "kind": "plan" }, "title": "Plan Auto (edited)" } }
    }));
    let edit_plan_text = extract_tool_text(&edit_plan);
    assert_eq!(
        edit_plan_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let complete_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "tasks_complete", "arguments": { "workspace": "ws_auto", "target": { "id": plan_id, "kind": "plan" }, "status": "ACTIVE" } }
    }));
    let complete_plan_text = extract_tool_text(&complete_plan);
    assert_eq!(
        complete_plan_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let focus_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws_auto", "target": { "id": task_id.clone(), "kind": "task" } } }
    }));
    let focus_task_text = extract_tool_text(&focus_task);
    assert_eq!(
        focus_task_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let note_focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "tasks_note", "arguments": { "workspace": "ws_auto", "step_id": step_id, "note": "focus ok" } }
    }));
    let note_focus_text = extract_tool_text(&note_focus);
    assert_eq!(
        note_focus_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn default_workspace_supports_portal_calls_without_workspace() {
    let mut server = Server::start_initialized_with_args(
        "default_workspace_supports_portal_calls_without_workspace",
        &["--workspace", "ws_default"],
    );

    let status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "status", "arguments": {} }
    }));
    let status_text = extract_tool_text_str(&status);
    assert_eq!(
        status_text.lines().count(),
        2,
        "status portal output must stay 2 lines"
    );
    assert!(
        status_text.starts_with("ready checkout="),
        "status must return a stable state summary"
    );
    assert!(
        status_text
            .lines()
            .nth(1)
            .unwrap_or("")
            .starts_with("tasks_snapshot"),
        "status should include a low-noise next action"
    );

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "plan_title": "DX default workspace plan",
                "task_title": "DX default workspace",
                "template": "principal-task",
                "resume_max_chars": 4000
            }
        }
    }));
    let start_text = extract_tool_text_str(&start);
    assert!(
        !start_text.starts_with("ERROR:"),
        "macro_start should succeed in default-workspace mode"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws_default" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let focused = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus");
    assert!(
        focused.starts_with("TASK-"),
        "focus must be set to the newly created task in the default workspace"
    );

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_snapshot", "arguments": {} }
    }));
    let snapshot_text = extract_tool_text_str(&snapshot);
    assert!(
        !snapshot_text.starts_with("ERROR:"),
        "snapshot portal call must succeed without explicit workspace/task when focus exists"
    );
}

#[test]
fn tasks_macro_start_uses_focused_plan_when_plan_is_omitted() {
    let mut server =
        Server::start_initialized("tasks_macro_start_uses_focused_plan_when_plan_is_omitted");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_focus_plan", "kind": "plan", "title": "Plan Focus" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();
    let plan_id_for_focus = plan_id.clone();

    let focus_set = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws_focus_plan", "plan": plan_id_for_focus } }
    }));
    let focus_set_text = extract_tool_text(&focus_set);
    assert_eq!(
        focus_set_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_focus_plan",
                "task_title": "Task under focused plan",
                "template": "basic-task",
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws_focus_plan" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus task id")
        .to_string();

    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws_focus_plan" } }
    }));
    let context_text = extract_tool_text(&context);
    let used_plan_id = context_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str()))
        })
        .and_then(|t| t.get("parent"))
        .and_then(|v| v.as_str())
        .expect("task parent plan id");
    assert_eq!(
        used_plan_id,
        plan_id.as_str(),
        "macro_start must reuse the focused plan when plan/plan_title is omitted"
    );
}

#[test]
fn tasks_macro_start_accepts_template_without_steps() {
    let mut server = Server::start_initialized("tasks_macro_start_accepts_template_without_steps");

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_macro_start", "arguments": { "workspace": "ws_tpl", "plan_title": "Plan Tpl", "task_title": "Task from template", "template": "basic-task" } }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_focus_get", "arguments": { "workspace": "ws_tpl" } }
    }));
    let focus_text = extract_tool_text(&focus);
    let task_id = focus_text
        .get("result")
        .and_then(|v| v.get("focus"))
        .and_then(|v| v.as_str())
        .expect("focus task id")
        .to_string();

    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_context", "arguments": { "workspace": "ws_tpl" } }
    }));
    let context_text = extract_tool_text(&context);
    let steps_count = context_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(task_id.as_str()))
        })
        .and_then(|t| t.get("steps_count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert_eq!(steps_count, 3, "basic-task template should create 3 steps");
}

#[test]
fn branchmind_focus_is_used_as_implicit_target() {
    let mut server = Server::start_initialized("branchmind_focus_is_used_as_implicit_target");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_focus", "kind": "plan", "title": "Plan Focus" } }
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
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_focus", "kind": "task", "parent": plan_id, "title": "Task Focus" } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let expected_branch = format!("task/{task_id}");

    let focus_set = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": { "workspace": "ws_focus", "target": { "id": task_id.clone(), "kind": "task" } } }
    }));
    let focus_set_text = extract_tool_text(&focus_set);
    assert_eq!(
        focus_set_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "notes_commit", "arguments": { "workspace": "ws_focus", "content": "focus note" } }
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
        Some(expected_branch.as_str())
    );

    let show = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "show", "arguments": { "workspace": "ws_focus", "doc_kind": "notes", "limit": 10 } }
    }));
    let show_text = extract_tool_text(&show);
    assert_eq!(
        show_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(expected_branch.as_str())
    );
    assert_eq!(
        show_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some("notes")
    );

    let graph = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "graph_query", "arguments": { "workspace": "ws_focus" } }
    }));
    let graph_text = extract_tool_text(&graph);
    assert_eq!(
        graph_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        graph_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(expected_branch.as_str())
    );
}
