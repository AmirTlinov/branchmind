#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn mcp_auto_init_allows_tools_list_without_notifications() {
    let mut server = Server::start("auto_init_allows_tools_list");

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

    // Auto-init path: tools/list should succeed even before notifications/initialized.
    let tools_list =
        server.request(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
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
    assert!(
        !names.is_empty(),
        "tools/list should return at least one tool"
    );
    // v1: advertised surface is fixed to the 10 portal tools.
    let expected = [
        "docs",
        "graph",
        "jobs",
        "open",
        "status",
        "system",
        "tasks",
        "think",
        "vcs",
        "workspace",
    ];
    assert_eq!(
        names.len(),
        expected.len(),
        "tools/list must return the v1 surface only (exactly 10 tools)"
    );
    for required in expected {
        assert!(
            names.iter().any(|n| n == required),
            "tools/list must include required tool: {required}"
        );
    }

    // Late notifications/initialized should be accepted.
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));
}

#[test]
fn mcp_resources_list_is_supported_and_empty() {
    let mut server = Server::start("resources_list_supported");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    let resources = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "resources/list",
        "params": {}
    }));
    let listed = resources
        .get("result")
        .and_then(|v| v.get("resources"))
        .and_then(|v| v.as_array())
        .expect("result.resources must be present");
    assert!(
        listed.is_empty(),
        "server should advertise an empty resources set by default"
    );
}

#[test]
fn mcp_resource_templates_list_is_supported_and_empty() {
    let mut server = Server::start("resource_templates_list_supported");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));

    // Compatibility: some clients probe this before notifications/initialized.
    let templates = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "resources/templates/list",
        "params": {}
    }));
    let listed = templates
        .get("result")
        .and_then(|v| v.get("resourceTemplates"))
        .and_then(|v| v.as_array())
        .expect("result.resourceTemplates must be present");
    assert!(
        listed.is_empty(),
        "server should advertise an empty resourceTemplates set by default"
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

    // v1: schema is discovered on-demand via system schema.get(cmd).
    // `tasks.plan.create` is the v1 cmd alias for the legacy tasks_create shape.
    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.plan.create" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let steps_items = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
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

    // v1: schema is discovered on-demand via system schema.get(cmd).
    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.focus.set" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let required = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("args_schema.required");
    let required = required
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(
        !required.iter().any(|v| *v == "task" || *v == "plan"),
        "tasks.focus.set must not require task/plan"
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

    // v1: schema is discovered on-demand via system schema.get(cmd).
    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.macro.start" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let properties = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .expect("args_schema.properties");
    assert!(
        properties.contains_key("template"),
        "tasks.macro.start must declare template"
    );
    assert!(
        properties.contains_key("think"),
        "tasks.macro.start must declare think passthrough"
    );

    let required = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("args_schema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("steps")),
        "tasks.macro.start must not require steps (template is allowed)"
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

    // v1: schema is discovered on-demand via system schema.get(cmd).
    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.macro.close.step" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let required = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("args_schema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("task")),
        "tasks.macro.close.step must not require task (focus-first)"
    );
}

#[test]
fn tools_schema_macro_close_step_declares_strict_override_shape() {
    let mut server = Server::start("tools_schema_macro_close_step_declares_strict_override_shape");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));

    // v1: schema is discovered on-demand via system schema.get(cmd).
    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.macro.close.step" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let input_schema = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.as_object())
        .expect("args_schema");

    let required = input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .expect("args_schema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("override")),
        "override must be optional at the top level"
    );

    let properties = input_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .expect("args_schema.properties");
    let override_schema = properties
        .get("override")
        .and_then(|v| v.as_object())
        .expect("override schema");
    assert_eq!(
        override_schema.get("type").and_then(|v| v.as_str()),
        Some("object")
    );

    let override_required = override_schema
        .get("required")
        .and_then(|v| v.as_array())
        .expect("override.required");
    assert!(
        override_required
            .iter()
            .any(|v| v.as_str() == Some("reason")),
        "override.reason must be required"
    );
    assert!(
        override_required.iter().any(|v| v.as_str() == Some("risk")),
        "override.risk must be required"
    );

    let override_props = override_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .expect("override.properties");
    let reason = override_props
        .get("reason")
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str());
    let risk = override_props
        .get("risk")
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str());
    assert_eq!(reason, Some("string"), "override.reason must be a string");
    assert_eq!(risk, Some("string"), "override.risk must be a string");
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

    // v1 portals: only the 10 tools are advertised, and workspace is optional at the tool level.
    for name in [
        "status",
        "open",
        "workspace",
        "tasks",
        "jobs",
        "think",
        "graph",
        "vcs",
        "docs",
        "system",
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
fn tools_schema_non_portal_tools_do_not_require_workspace_when_default_is_configured() {
    // v1 contract: workspace is part of the portal envelope (not the cmd args).
    // When a default workspace is configured, schema.get should not force callers
    // to include workspace in cmd args.
    let mut server = Server::start_initialized_with_args(
        "tools_schema_non_portal_tools_do_not_require_workspace",
        &["--workspace", "ws_default"],
    );

    let schema = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "schema.get", "args": { "cmd": "tasks.plan.create" } }
        }
    }));
    let schema_text = extract_tool_text(&schema);
    assert_eq!(
        schema_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed"
    );
    let required = schema_text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .expect("args_schema.required");
    assert!(
        !required.iter().any(|v| v.as_str() == Some("workspace")),
        "cmd args must not require workspace in v1"
    );
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

    // v1: tools/list always advertises the fixed 10-tool portal surface (toolset does not change the list).
    let mut names = tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    names.sort();

    let expected = [
        "docs",
        "graph",
        "jobs",
        "open",
        "status",
        "system",
        "tasks",
        "think",
        "vcs",
        "workspace",
    ];
    assert_eq!(
        names.len(),
        expected.len(),
        "v1 surface must be exactly 10 tools"
    );
    for required in expected {
        assert!(
            names.iter().any(|n| n == required),
            "tools/list must include {required}"
        );
    }
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
    assert_eq!(
        daily_tools.len(),
        10,
        "v1 surface must be exactly 10 tools (even in daily toolset)"
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
    assert_eq!(
        full_tools.len(),
        10,
        "v1 surface must remain exactly 10 tools even when overriding toolset"
    );

    let mut daily_names = daily_tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    daily_names.sort();
    let mut full_names = full_tools
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    full_names.sort();
    assert_eq!(
        daily_names, full_names,
        "tools/list must not drift across toolset overrides in v1"
    );
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

    // v1: tools/list always advertises the fixed 10-tool portal surface.
    assert_eq!(
        tools.len(),
        10,
        "v1 surface must be exactly 10 tools (even in core toolset)"
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

    // v1: explicit portal call (workspace lives in envelope; legacy tools receive injected workspace).
    let context = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": { "workspace": "ws_auto", "op": "call", "cmd": "tasks.context", "args": {} }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.plan.create",
                "args": { "kind": "plan", "title": "Plan Auto" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.plan.create",
                "args": {
                    "kind": "task",
                    "parent": plan_id.clone(),
                    "title": "Task Auto",
                    "steps": [ { "title": "Step 1", "success_criteria": ["ok"] } ]
                }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.focus.set",
                "args": { "plan": plan_id }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.radar",
                "args": { "task": task_id.clone() }
            }
        }
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
        "params": {
            "name": "vcs",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "vcs.notes.commit",
                "args": { "target": task_id, "content": "auto-init ok" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.edit",
                "args": { "plan": plan_id.clone(), "title": "Plan Auto (edited)" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.complete",
                "args": { "plan": plan_id, "status": "ACTIVE" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.focus.set",
                "args": { "task": task_id.clone() }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_auto",
                "op": "call",
                "cmd": "tasks.note",
                "args": { "task": task_id, "step_id": step_id, "note": "focus ok" }
            }
        }
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
    let status_text = extract_tool_text(&status);
    assert_eq!(
        status_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "status portal call must succeed without explicit workspace when default is configured"
    );
    assert_eq!(
        status_text
            .get("result")
            .and_then(|v| v.get("workspace"))
            .and_then(|v| v.as_str()),
        Some("ws_default"),
        "status must resolve workspace from --workspace default"
    );

    let start = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                // Portal-grade macros default to BM-L1 lines; request JSON here so the test can
                // assert on structured success fields deterministically.
                "fmt": "json",
                "op": "call",
                "cmd": "tasks.macro.start",
                "args": {
                    "plan_title": "DX default workspace plan",
                    "task_title": "DX default workspace",
                    "template": "principal-task",
                    "resume_max_chars": 4000
                }
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
        "params": {
            "name": "tasks",
            "arguments": { "workspace": "ws_default", "op": "call", "cmd": "tasks.focus.get", "args": {} }
        }
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
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": {} } }
    }));
    let snapshot_text = extract_tool_text(&snapshot);
    assert!(
        snapshot_text.get("success").and_then(|v| v.as_bool()) == Some(true),
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
fn tasks_macro_start_accepts_plan_id_with_matching_plan_title() {
    let mut server =
        Server::start_initialized("tasks_macro_start_accepts_plan_id_with_matching_plan_title");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_match", "kind": "plan", "title": "Plan Match" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_match",
                "plan": plan_id,
                "plan_title": "Plan Match",
                "task_title": "Task under plan",
                "template": "basic-task",
                "resume_max_chars": 4000
            }
        }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start must accept matching plan+plan_title"
    );
}

#[test]
fn tasks_macro_start_accepts_plan_title_in_plan_field_when_not_plan_id() {
    let mut server = Server::start_initialized(
        "tasks_macro_start_accepts_plan_title_in_plan_field_when_not_plan_id",
    );

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_plan_alias",
                "plan": "Inbox",
                "task_title": "Task under Inbox",
                "template": "basic-task",
                "resume_max_chars": 4000
            }
        }
    }));
    let text = extract_tool_text_str(&started);
    assert!(
        !text.starts_with("ERROR:"),
        "macro_start must accept plan title in plan field"
    );
    assert!(
        text.contains("Task under Inbox"),
        "portal output should reference the created task title"
    );
}

#[test]
fn tasks_macro_start_rejects_plan_id_with_mismatching_plan_title() {
    let mut server =
        Server::start_initialized("tasks_macro_start_rejects_plan_id_with_mismatching_plan_title");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": { "workspace": "ws_mismatch", "kind": "plan", "title": "Plan Actual" } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_macro_start",
            "arguments": {
                "workspace": "ws_mismatch",
                "plan": plan_id,
                "plan_title": "Plan Different",
                "task_title": "Task under plan",
                "template": "basic-task",
                "resume_max_chars": 4000
            }
        }
    }));
    let text = extract_tool_text_str(&started);
    assert!(
        text.starts_with("ERROR: INVALID_INPUT"),
        "macro_start must reject mismatching plan_title"
    );
    assert!(
        text.contains("plan_title"),
        "error should mention plan_title mismatch"
    );
}

#[test]
fn invalid_input_errors_include_hints_in_json_payloads() {
    let mut server =
        Server::start_initialized("invalid_input_errors_include_hints_in_json_payloads");

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": { "workspace": "ws_invalid", "op": "call", "cmd": "tasks.plan.create", "args": {} }
        }
    }));
    let text = extract_tool_text(&resp);
    let err = text
        .get("error")
        .and_then(|v| v.as_object())
        .expect("error object");
    assert_eq!(
        err.get("code").and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );

    // v1 UX: INVALID_INPUT returns deterministic auto-actions:
    // - system schema.get(cmd)
    // - a minimal valid call example
    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions[]");
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("system")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("schema.get")
        }),
        "INVALID_INPUT must include schema.get action"
    );
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("tasks")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("call")
        }),
        "INVALID_INPUT must include example call action"
    );
}

#[test]
fn branchmind_focus_is_used_as_implicit_target() {
    let mut server = Server::start_initialized("branchmind_focus_is_used_as_implicit_target");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_focus",
                "op": "call",
                "cmd": "tasks.plan.create",
                "args": { "kind": "plan", "title": "Plan Focus" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_focus",
                "op": "call",
                "cmd": "tasks.plan.create",
                "args": { "kind": "task", "parent": plan_id, "title": "Task Focus" }
            }
        }
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
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_focus",
                "op": "call",
                "cmd": "tasks.focus.set",
                "args": { "task": task_id.clone() }
            }
        }
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
        "params": {
            "name": "vcs",
            "arguments": {
                "workspace": "ws_focus",
                "op": "call",
                "cmd": "vcs.notes.commit",
                "args": { "content": "focus note" }
            }
        }
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
        "params": {
            "name": "docs",
            "arguments": {
                "workspace": "ws_focus",
                "op": "call",
                "cmd": "docs.show",
                "args": { "doc_kind": "notes", "limit": 10 }
            }
        }
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
        "params": {
            "name": "graph",
            "arguments": { "workspace": "ws_focus", "op": "call", "cmd": "graph.query", "args": {} }
        }
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
