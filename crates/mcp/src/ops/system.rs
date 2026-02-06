#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, BudgetPolicy, CommandRegistry, CommandSpec, ConfirmLevel, DocRef,
    Envelope, OpError, OpResponse, Safety, SchemaSource, Stability, Tier, ToolName,
    schema_bundle_for_cmd,
};
use serde_json::{Value, json};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // system.schema.get (custom)
    specs.push(CommandSpec {
        cmd: "system.schema.get".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.schema.get".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": { "cmd": { "type": "string" } },
                "required": ["cmd"]
            }),
            example_minimal_args: json!({ "cmd": "tasks.snapshot" }),
        },
        op_aliases: vec!["schema.get".to_string()],
        handler_name: None,
        handler: Some(handle_schema_get),
    });

    // system.ops.summary (custom)
    specs.push(CommandSpec {
        cmd: "system.ops.summary".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.ops.summary".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["ops.summary".to_string()],
        handler_name: None,
        handler: Some(handle_ops_summary),
    });

    // system.cmd.list (custom)
    specs.push(CommandSpec {
        cmd: "system.cmd.list".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.cmd.list".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "prefix": { "type": "string" },
                    "include_hidden": { "type": "boolean", "description": "List all registered cmds (full registry; ignores the kernel/toolset filter)." },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({ "prefix": "tasks." }),
        },
        op_aliases: vec!["cmd.list".to_string()],
        handler_name: None,
        handler: Some(handle_cmd_list),
    });

    // system.tutorial (custom)
    specs.push(CommandSpec {
        cmd: "system.tutorial".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.tutorial".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: Vec::new(),
        handler_name: None,
        handler: Some(handle_tutorial),
    });

    // Minimal system tools exposed via cmd=system.<name>.
    for handler_name in ["storage", "init", "help", "skill", "diagnostics"] {
        let tier = match handler_name {
            "storage" | "diagnostics" => Tier::Internal,
            _ => Tier::Advanced,
        };
        specs.push(CommandSpec {
            cmd: format!("system.{handler_name}"),
            domain_tool: ToolName::SystemOps,
            tier,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: false,
                confirm_level: ConfirmLevel::None,
                idempotent: true,
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Handler,
            op_aliases: Vec::new(),
            handler_name: Some(handler_name.to_string()),
            handler: None,
        });
    }
}

fn handle_schema_get(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={cmd:\"tasks.snapshot\"}".to_string()),
            },
        );
    };
    let Some(cmd_raw) = args_obj.get("cmd").and_then(|v| v.as_str()) else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "cmd is required".to_string(),
                recovery: Some("Provide args.cmd".to_string()),
            },
        );
    };
    let cmd = match crate::ops::normalize_cmd(cmd_raw) {
        Ok(v) => v,
        Err(msg) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: format!("cmd {msg}"),
                    recovery: None,
                },
            );
        }
    };

    let bundle = match schema_bundle_for_cmd(&cmd, env.workspace.as_deref()) {
        Ok(v) => v,
        Err(err) => return OpResponse::error(env.cmd.clone(), err),
    };

    // UX: schema-on-demand must be fail-open at runtime.
    //
    // Docs drift is a CI/maintainer concern (we keep hard guards in tests). At runtime the agent
    // should always get a schema bundle even if local docs are missing/unavailable.
    OpResponse::success(
        env.cmd.clone(),
        json!({
            "cmd": bundle.cmd,
            "args_schema": bundle.args_schema,
            "example_minimal_args": bundle.example_minimal_args,
            "example_valid_call": bundle.example_valid_call,
            "doc_ref": { "path": bundle.doc_ref.path, "anchor": bundle.doc_ref.anchor },
            "default_budget_profile": bundle.default_budget_profile.as_str(),
            "tier": bundle.tier.as_str(),
            "stability": bundle.stability.as_str(),
            "safety": {
                "destructive": bundle.safety.destructive,
                "confirm_level": bundle.safety.confirm_level.as_str(),
                "idempotent": bundle.safety.idempotent
            }
        }),
    )
}

fn handle_ops_summary(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let registry = CommandRegistry::global();

    // Surface: 10 tools (fixed by contract).
    let surface = crate::tools_v1::tool_definitions();
    let mut surface_names = surface
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    surface_names.sort();

    // Count cmd by domain prefix (tasks.*, think.*, ...).
    let mut cmd_by_domain = std::collections::BTreeMap::<String, usize>::new();
    for cmd in registry.list_cmds() {
        let domain = cmd.split('.').next().unwrap_or("cmd").to_string();
        *cmd_by_domain.entry(domain).or_insert(0) += 1;
    }

    // Count golden ops as advertised in tools/list (inputSchema.properties.op.enum),
    // and verify they are wired to the registry aliases (no unplugged ops).
    let mut golden_ops_total: usize = 0;
    let mut golden_ops_by_tool = std::collections::BTreeMap::<String, usize>::new();
    let mut unplugged = Vec::<String>::new();

    for tool in surface.iter() {
        let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(op_enum) = tool
            .get("inputSchema")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.get("op"))
            .and_then(|v| v.get("enum"))
            .and_then(|v| v.as_array())
        else {
            continue;
        };

        let tool_name = match name {
            "workspace" => Some(ToolName::WorkspaceOps),
            "tasks" => Some(ToolName::TasksOps),
            "jobs" => Some(ToolName::JobsOps),
            "think" => Some(ToolName::ThinkOps),
            "graph" => Some(ToolName::GraphOps),
            "vcs" => Some(ToolName::VcsOps),
            "docs" => Some(ToolName::DocsOps),
            "system" => Some(ToolName::SystemOps),
            _ => None,
        };

        let mut count = 0usize;
        for op in op_enum.iter().filter_map(|v| v.as_str()) {
            if op == "call" {
                continue;
            }
            count += 1;
            golden_ops_total += 1;
            if let Some(tool_name) = tool_name
                && registry.find_by_alias(tool_name, op).is_none()
            {
                unplugged.push(format!("{name}.{op}"));
            }
        }
        if count > 0 {
            golden_ops_by_tool.insert(name.to_string(), count);
        }
    }

    unplugged.sort();
    unplugged.dedup();

    let mut resp = OpResponse::success(
        env.cmd.clone(),
        json!({
            "surface": {
                "tools": {
                    "count": surface_names.len(),
                    "names": surface_names
                },
                "golden_ops": {
                    "count": golden_ops_total,
                    "by_tool": golden_ops_by_tool,
                    "unplugged": unplugged
                }
            },
            "registry": {
                "cmd": {
                    "count": registry.list_cmds().len(),
                    "by_domain": cmd_by_domain
                }
            }
        }),
    );

    if let Some(arr) = resp
        .result
        .get("surface")
        .and_then(|v| v.get("golden_ops"))
        .and_then(|v| v.get("unplugged"))
        .and_then(|v| v.as_array())
        && !arr.is_empty()
    {
        resp.warnings.push(crate::warning(
            "UNPLUGGED_OPS",
            "Some ops are advertised in tools/list but not wired to the cmd registry.",
            "Fix tools_v1/definitions.rs or add missing op_aliases in ops/* registry.",
        ));
    }

    resp
}

fn is_kernel_cmd(spec: &CommandSpec) -> bool {
    // Kernel surface should stay *small* and stable. It is what agents should discover first.
    //
    // Rule:
    // - Any golden op (cmd with at least one op alias) is kernel.
    // - Plus a curated set of workflow macros / call-only navigators.
    if !spec.op_aliases.is_empty() {
        return true;
    }

    matches!(
        spec.cmd.as_str(),
        // Task workflow (call-only macros + snapshot).
        "tasks.macro.start"
            | "tasks.macro.close.step"
            | "tasks.macro.delegate"
            | "tasks.macro.finish"
            | "tasks.snapshot"
            | "tasks.lint"
            // Thinking primitives (handlers are kernel even if not golden ops).
            | "think.card"
            | "think.playbook"
            | "think.macro.anchor.note"
            // Anchor navigation (meaning map).
            | "think.anchor.list"
            | "think.anchor.snapshot"
            // Deterministic discovery and onboarding.
            | "system.schema.get"
            | "system.help"
            | "system.tutorial"
            | "system.skill"
            | "system.ops.summary"
            | "system.cmd.list"
    )
}

fn handle_cmd_list(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let prefix = args_obj
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let include_hidden = args_obj
        .get("include_hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let offset = args_obj.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args_obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let registry = CommandRegistry::global();
    let mut cmds = if include_hidden {
        registry.list_cmds()
    } else {
        let mut out = registry
            .specs()
            .iter()
            .filter(|spec| spec.tier.allowed_in_toolset(server.toolset))
            .filter(|spec| is_kernel_cmd(spec))
            .map(|spec| spec.cmd.clone())
            .collect::<Vec<_>>();
        out.sort();
        out
    };
    if let Some(prefix) = prefix.as_deref() {
        cmds.retain(|c| c.starts_with(prefix));
    }
    let total = cmds.len();

    let page = cmds
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(page.len()) < total;
    let next_cursor = if has_more {
        Some(offset.saturating_add(page.len()) as i64)
    } else {
        None
    };

    OpResponse::success(
        env.cmd.clone(),
        json!({
            "cmds": page,
            "pagination": {
                "offset": offset,
                "limit": limit,
                "total": total,
                "has_more": has_more,
                "next_cursor": next_cursor
            }
        }),
    )
}

fn handle_tutorial(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let limit = args_obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(3)
        .clamp(1, 5);
    let max_chars = args_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let mut summary = "Пошаговый старт: 1) status → контекст, 2) tasks.macro.start → первая задача, 3) tasks.snapshot → фокус.".to_string();
    let mut truncated = false;
    if let Some(max_chars) = max_chars {
        let (max_chars, clamped) = crate::clamp_budget_max(max_chars);
        let suffix = "...";
        if summary.len() > max_chars {
            let budget = max_chars.saturating_sub(suffix.len());
            summary = crate::truncate_string_bytes(&summary, budget) + suffix;
            truncated = true;
        }
        if clamped {
            truncated = true;
        }
    }

    let workspace = env.workspace.as_deref();
    let mut steps = vec![
        json!({
            "id": "status",
            "title": "Проверить состояние",
            "tool": "status",
            "purpose": "Быстрый статус workspace и NextEngine.",
            "action_id": "tutorial::status"
        }),
        json!({
            "id": "start-task",
            "title": "Создать первую задачу",
            "tool": "tasks",
            "cmd": "tasks.macro.start",
            "purpose": "Создаёт задачу по базовому шаблону.",
            "action_id": "tutorial::tasks.macro.start"
        }),
        json!({
            "id": "snapshot",
            "title": "Сделать снимок",
            "tool": "tasks",
            "cmd": "tasks.snapshot",
            "purpose": "Закрепить фокус и получить следующий шаг.",
            "action_id": "tutorial::tasks.snapshot"
        }),
    ];
    if limit < steps.len() {
        steps.truncate(limit);
        truncated = true;
    }

    let mut actions = Vec::<Action>::new();
    for (idx, step) in steps.iter().enumerate() {
        let action_id = step
            .get("action_id")
            .and_then(|v| v.as_str())
            .unwrap_or("tutorial::step")
            .to_string();
        let priority = match idx {
            0 => ActionPriority::High,
            1 => ActionPriority::Medium,
            _ => ActionPriority::Low,
        };
        let tool = step
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("system")
            .to_string();

        let args = match step.get("id").and_then(|v| v.as_str()) {
            Some("status") => {
                let mut obj = serde_json::Map::new();
                if let Some(ws) = workspace {
                    obj.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                obj.insert(
                    "budget_profile".to_string(),
                    Value::String("portal".to_string()),
                );
                obj.insert("view".to_string(), Value::String("compact".to_string()));
                Value::Object(obj)
            }
            Some("start-task") => {
                let mut obj = serde_json::Map::new();
                if let Some(ws) = workspace {
                    obj.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                obj.insert("op".to_string(), Value::String("call".to_string()));
                obj.insert(
                    "cmd".to_string(),
                    Value::String("tasks.macro.start".to_string()),
                );
                obj.insert(
                    "args".to_string(),
                    json!({
                        "task_title": "First task",
                        "template": "basic-task"
                    }),
                );
                obj.insert(
                    "budget_profile".to_string(),
                    Value::String("portal".to_string()),
                );
                obj.insert("view".to_string(), Value::String("compact".to_string()));
                Value::Object(obj)
            }
            Some("snapshot") => {
                let mut obj = serde_json::Map::new();
                if let Some(ws) = workspace {
                    obj.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                obj.insert("op".to_string(), Value::String("call".to_string()));
                obj.insert(
                    "cmd".to_string(),
                    Value::String("tasks.snapshot".to_string()),
                );
                obj.insert("args".to_string(), json!({ "view": "smart" }));
                obj.insert(
                    "budget_profile".to_string(),
                    Value::String("portal".to_string()),
                );
                obj.insert("view".to_string(), Value::String("compact".to_string()));
                Value::Object(obj)
            }
            _ => json!({}),
        };

        let why = step
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("Guided onboarding step.")
            .to_string();

        actions.push(Action {
            action_id,
            priority,
            tool,
            args,
            why,
            risk: "Низкий".to_string(),
        });
    }

    let result = json!({
        "title": "Guided onboarding",
        "summary": summary,
        "steps": steps,
        "truncated": truncated
    });

    let mut resp = OpResponse::success(env.cmd.clone(), result);
    resp.actions = actions;
    resp
}
