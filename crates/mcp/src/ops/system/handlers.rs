#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, CommandRegistry, CommandSpec, Envelope, OpError, OpResponse,
    QUICKSTART_DEFAULT_PORTAL, SchemaSource, Tier, ToolName, handler_to_op_response,
    quickstart_curated_portals_joined, quickstart_example_env, quickstart_recipes_for_portal,
    schema_bundle_for_cmd,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn handle_schema_get(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "UNKNOWN_ARG".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={cmd:\"tasks.snapshot\"}".to_string()),
            },
        );
    };
    let Some(cmd_raw) = args_obj.get("cmd").and_then(|v| v.as_str()) else {
        let portal = args_obj
            .get("portal")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let prefix = args_obj
            .get("prefix")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let q = args_obj
            .get("q")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut resp = OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "cmd is required".to_string(),
                recovery: Some(
                    "Provide args.cmd (or use system(op=schema.list) to discover cmds)."
                        .to_string(),
                ),
            },
        );

        // DX: users often try `system(op=schema.get args={portal:\"tasks\"})`. Suggest schema.list.
        if portal.is_some() || prefix.is_some() || q.is_some() {
            let mut list_args = serde_json::Map::new();
            if let Some(portal) = portal.as_deref() {
                list_args.insert("portal".to_string(), Value::String(portal.to_string()));
            }
            if let Some(prefix) = prefix.as_deref() {
                list_args.insert("prefix".to_string(), Value::String(prefix.to_string()));
            }
            if let Some(q) = q.as_deref() {
                list_args.insert("q".to_string(), Value::String(q.to_string()));
            }
            list_args.insert("limit".to_string(), Value::Number(20.into()));

            let mut list_env = serde_json::Map::new();
            if let Some(ws) = env.workspace.as_deref() {
                list_env.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            list_env.insert("op".to_string(), Value::String("schema.list".to_string()));
            list_env.insert("args".to_string(), Value::Object(list_args));
            list_env.insert(
                "budget_profile".to_string(),
                Value::String("portal".to_string()),
            );
            list_env.insert(
                "portal_view".to_string(),
                Value::String("compact".to_string()),
            );

            resp.actions.push(Action {
                action_id: "recover.schema.list::system.schema.get".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::SystemOps.as_str().to_string(),
                args: Value::Object(list_env),
                why: "Сначала найти cmd через schema.list (portal/prefix/q).".to_string(),
                risk: "Низкий".to_string(),
            });

            let mut get_env = serde_json::Map::new();
            if let Some(ws) = env.workspace.as_deref() {
                get_env.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            get_env.insert("op".to_string(), Value::String("schema.get".to_string()));
            get_env.insert("args".to_string(), json!({ "cmd": "tasks.snapshot" }));
            get_env.insert(
                "budget_profile".to_string(),
                Value::String("portal".to_string()),
            );
            get_env.insert(
                "portal_view".to_string(),
                Value::String("compact".to_string()),
            );

            resp.actions.push(Action {
                action_id: "recover.example.schema.get::tasks.snapshot".to_string(),
                priority: ActionPriority::Medium,
                tool: ToolName::SystemOps.as_str().to_string(),
                args: Value::Object(get_env),
                why: "Пример: получить точную схему для tasks.snapshot.".to_string(),
                risk: "Низкий".to_string(),
            });
        }

        return resp;
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

fn tool_from_portal(portal: &str) -> Option<ToolName> {
    match portal.trim().to_ascii_lowercase().as_str() {
        "status" => Some(ToolName::Status),
        "open" => Some(ToolName::Open),
        "workspace" => Some(ToolName::WorkspaceOps),
        "tasks" => Some(ToolName::TasksOps),
        "jobs" => Some(ToolName::JobsOps),
        "think" => Some(ToolName::ThinkOps),
        "graph" => Some(ToolName::GraphOps),
        "vcs" => Some(ToolName::VcsOps),
        "docs" => Some(ToolName::DocsOps),
        "system" => Some(ToolName::SystemOps),
        _ => None,
    }
}

struct SchemaRequiredHints {
    required: Vec<String>,
    required_any_of: Vec<Vec<String>>,
}

fn strip_workspace_from_schema(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    if let Some(required) = obj.get_mut("required").and_then(|v| v.as_array_mut()) {
        required.retain(|v| v.as_str() != Some("workspace"));
    }
    if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
        props.remove("workspace");
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = obj.get_mut(key).and_then(|v| v.as_array_mut()) {
            for variant in variants {
                strip_workspace_from_schema(variant);
            }
        }
    }
}

fn required_fields_from_schema(schema: &Value) -> Vec<String> {
    let mut out = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn schema_required_hints_for_spec(spec: &CommandSpec) -> SchemaRequiredHints {
    let schema = match &spec.schema {
        SchemaSource::Custom { args_schema, .. } => Some(args_schema.clone()),
        SchemaSource::Handler => spec
            .handler_name
            .as_deref()
            .and_then(crate::ops::schema::handler_input_schema),
    };
    let Some(mut schema) = schema else {
        return SchemaRequiredHints {
            required: Vec::new(),
            required_any_of: Vec::new(),
        };
    };
    strip_workspace_from_schema(&mut schema);

    let mut required_set = required_fields_from_schema(&schema)
        .into_iter()
        .collect::<BTreeSet<_>>();

    // allOf: all branches apply, so required fields are additive.
    if let Some(branches) = schema.get("allOf").and_then(|v| v.as_array()) {
        for branch in branches {
            for field in required_fields_from_schema(branch) {
                required_set.insert(field);
            }
        }
    }

    let mut required_any_of = Vec::<Vec<String>>::new();
    for key in ["oneOf", "anyOf"] {
        let Some(branches) = schema.get(key).and_then(|v| v.as_array()) else {
            continue;
        };
        let mut branch_sets = Vec::<BTreeSet<String>>::new();
        for branch in branches {
            let req = required_fields_from_schema(branch);
            if !req.is_empty() {
                branch_sets.push(req.into_iter().collect());
            }
        }
        if branch_sets.is_empty() {
            continue;
        }

        // Common across every branch is truly required.
        let mut common = branch_sets[0].clone();
        for set in branch_sets.iter().skip(1) {
            common = common.intersection(set).cloned().collect::<BTreeSet<_>>();
        }
        for field in &common {
            required_set.insert(field.clone());
        }

        // Branch-specific required fields are exposed via required_any_of.
        for set in branch_sets {
            let mut alt = set
                .into_iter()
                .filter(|field| !required_set.contains(field))
                .collect::<Vec<_>>();
            alt.sort();
            alt.dedup();
            if !alt.is_empty() {
                required_any_of.push(alt);
            }
        }
    }

    // Deterministic dedupe for required_any_of alternatives.
    let mut seen = BTreeSet::<String>::new();
    required_any_of.retain(|alt| seen.insert(alt.join("\u{1f}")));
    required_any_of.sort();

    SchemaRequiredHints {
        required: required_set.into_iter().collect(),
        required_any_of,
    }
}

pub(crate) fn handle_schema_list(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let portal = args_obj
        .get("portal")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let prefix = args_obj
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let q = args_obj
        .get("q")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let mode = args_obj
        .get("mode")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "golden".to_string());
    let include_detailed = match mode.as_str() {
        "golden" => false,
        "all" => true,
        // Backward-compatible aliases.
        "names" => false,
        "compact" => true,
        _ => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "mode must be one of: golden|all".to_string(),
                    recovery: Some("Use mode=\"golden\" (default) or mode=\"all\".".to_string()),
                },
            );
        }
    };

    let offset = args_obj.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args_obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let limit = limit.clamp(1, 50);

    let portal_tool = match portal.as_deref() {
        Some(portal) => match tool_from_portal(portal) {
            Some(tool) => Some(tool),
            None => {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: "unknown portal".to_string(),
                        recovery: Some(
                            "Use system(op=tools.list) to see portal names.".to_string(),
                        ),
                    },
                );
            }
        },
        None => None,
    };

    let registry = CommandRegistry::global();
    let mut hits = Vec::<&CommandSpec>::new();
    for spec in registry.specs() {
        if !include_detailed && spec.tier != Tier::Gold {
            continue;
        }
        if let Some(tool) = portal_tool
            && spec.domain_tool != tool
        {
            continue;
        }
        if let Some(prefix) = prefix.as_deref()
            && !spec.cmd.starts_with(prefix)
        {
            continue;
        }
        if let Some(q) = q.as_deref()
            && !spec.cmd.to_ascii_lowercase().contains(q)
        {
            continue;
        }
        hits.push(spec);
    }
    hits.sort_by(|a, b| a.cmd.cmp(&b.cmd));
    let total = hits.len();

    let page = hits
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|spec| {
            if include_detailed {
                let hints = schema_required_hints_for_spec(spec);
                json!({
                    "cmd": spec.cmd.clone(),
                    "tool": spec.domain_tool.as_str(),
                    "op_aliases": spec.op_aliases.clone(),
                    "required": hints.required,
                    "required_any_of": hints.required_any_of,
                    "doc_ref": { "path": spec.doc_ref.path.clone(), "anchor": spec.doc_ref.anchor.clone() }
                })
            } else {
                json!({
                    "cmd": spec.cmd.clone(),
                    "tool": spec.domain_tool.as_str(),
                    "doc_ref": { "path": spec.doc_ref.path.clone(), "anchor": spec.doc_ref.anchor.clone() }
                })
            }
        })
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
            "schemas": page,
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

pub(crate) fn handle_tools_list(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let mut tools = Vec::<Value>::new();
    for t in crate::tools_v1::tool_definitions() {
        let name = t
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("-")
            .to_string();
        let description = t
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ops = t
            .get("inputSchema")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.get("op"))
            .and_then(|v| v.get("enum"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        tools.push(json!({
            "tool": name,
            "description": description,
            "ops": ops
        }));
    }
    tools.sort_by(|a, b| {
        a.get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("tool").and_then(|v| v.as_str()).unwrap_or(""))
    });

    let workspace = env.workspace.as_deref();
    let mut examples = Vec::<Value>::new();

    let mut tutorial = serde_json::Map::new();
    if let Some(ws) = workspace {
        tutorial.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    tutorial.insert("op".to_string(), Value::String("tutorial".to_string()));
    tutorial.insert("args".to_string(), Value::Object(serde_json::Map::new()));
    tutorial.insert(
        "budget_profile".to_string(),
        Value::String("portal".to_string()),
    );
    tutorial.insert(
        "portal_view".to_string(),
        Value::String("compact".to_string()),
    );
    examples.push(json!({
        "title": "Guided onboarding",
        "tool": "system",
        "args": Value::Object(tutorial)
    }));

    examples.push(json!({
        "title": "Quickstart recipes for a portal",
        "tool": "system",
        "args": quickstart_example_env(workspace, QUICKSTART_DEFAULT_PORTAL)
    }));

    let mut schema_list = serde_json::Map::new();
    if let Some(ws) = workspace {
        schema_list.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    schema_list.insert("op".to_string(), Value::String("schema.list".to_string()));
    schema_list.insert(
        "args".to_string(),
        json!({ "portal": "tasks", "limit": 20 }),
    );
    schema_list.insert(
        "budget_profile".to_string(),
        Value::String("portal".to_string()),
    );
    schema_list.insert(
        "portal_view".to_string(),
        Value::String("compact".to_string()),
    );
    examples.push(json!({
        "title": "List schemas for a portal",
        "tool": "system",
        "args": Value::Object(schema_list)
    }));

    let mut schema_get = serde_json::Map::new();
    if let Some(ws) = workspace {
        schema_get.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    schema_get.insert("op".to_string(), Value::String("schema.get".to_string()));
    schema_get.insert("args".to_string(), json!({ "cmd": "tasks.snapshot" }));
    schema_get.insert(
        "budget_profile".to_string(),
        Value::String("portal".to_string()),
    );
    schema_get.insert(
        "portal_view".to_string(),
        Value::String("compact".to_string()),
    );
    examples.push(json!({
        "title": "Get exact schema for a cmd",
        "tool": "system",
        "args": Value::Object(schema_get)
    }));

    OpResponse::success(
        env.cmd.clone(),
        json!({
            "tools": tools,
            "examples": examples,
            "quickstart_schema_hint": {
                "version": 1,
                "schema_cmd": "system.quickstart",
                "defaults": {
                    "json_path": "result.defaults",
                    "keys": ["checkout_branch", "default_branch"]
                },
                "recipe_uses_defaults": {
                    "json_path": "result.recipes[].uses_defaults",
                    "enum": ["checkout_branch", "default_branch"]
                }
            },
            "notes": [
                "ops are tool-level shortcuts; long-tail operations use op=call + cmd.",
                "Use system(op=schema.list) → system(op=schema.get) for exact cmd arguments.",
                format!("Quickstart curated portals: {}", quickstart_curated_portals_joined()),
                "Quickstart recipes include result.defaults + recipes[].uses_defaults (UI badges)."
            ]
        }),
    )
}

pub(crate) fn handle_quickstart(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={portal:\"tasks\"}".to_string()),
            },
        );
    };

    let portal = args_obj
        .get("portal")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if portal.is_empty() {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "portal is required".to_string(),
                recovery: Some("Provide args={portal:\"tasks\"}".to_string()),
            },
        );
    }

    let Some(portal_tool) = tool_from_portal(&portal) else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "unknown portal".to_string(),
                recovery: Some("Use system(op=tools.list) to see portal names.".to_string()),
            },
        );
    };

    let limit = args_obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(5)
        .clamp(1, 5);
    let workspace = env.workspace.as_deref();
    let workspace_selected_source = if workspace.is_none() {
        "none"
    } else if server.workspace_override.as_deref() == workspace {
        "workspace_override"
    } else if server.default_workspace.as_deref() == workspace {
        "default_workspace"
    } else {
        "explicit_argument"
    };

    let default_branch = server.store.default_branch_name();
    let checkout_branch = workspace
        .and_then(|ws| crate::WorkspaceId::try_new(ws.to_string()).ok())
        .and_then(|ws_id| server.store.branch_checkout_get(&ws_id).ok().flatten());

    let curated = quickstart_recipes_for_portal(
        portal_tool,
        workspace,
        checkout_branch.as_deref(),
        default_branch,
    );
    let mut recipes = Vec::<Value>::new();
    let mut actions = Vec::<Action>::new();
    let mut truncated = false;

    if curated.is_empty() {
        // Minimal fallback: succeed but return only metadata (no actions).
        recipes.push(json!({
            "id": "unsupported",
            "title": "No curated recipes yet",
            "purpose": "This portal does not have curated quickstart recipes yet.",
            "tool": portal_tool.as_str(),
            "uses_defaults": []
        }));
    } else {
        if curated.len() > limit {
            truncated = true;
        }
        for (idx, recipe) in curated.into_iter().take(limit).enumerate() {
            let tool = recipe.tool.as_str();
            let action_id = format!("quickstart::{tool}::{}", recipe.id);
            let priority = match idx {
                0 => ActionPriority::High,
                1 => ActionPriority::Medium,
                _ => ActionPriority::Low,
            };
            recipes.push(json!({
                "id": recipe.id,
                "title": recipe.title,
                "purpose": recipe.purpose,
                "tool": tool,
                "action_id": action_id,
                "uses_defaults": recipe.uses_defaults
            }));
            actions.push(Action {
                action_id,
                priority,
                tool: tool.to_string(),
                args: recipe.args,
                why: recipe.purpose.to_string(),
                risk: recipe.risk.to_string(),
            });
        }
    }

    let result = json!({
        "title": format!("Quickstart — {}", portal_tool.as_str()),
        "portal": portal_tool.as_str(),
        "workspace_selected": workspace,
        "workspace_selected_source": workspace_selected_source,
        "defaults": {
            "default_branch": default_branch,
            "checkout_branch": checkout_branch
        },
        "recipes": recipes,
        "truncated": truncated
    });

    let mut resp = OpResponse::success(env.cmd.clone(), result);
    resp.actions = actions;
    resp
}

fn append_actions_dedupe(dst: &mut Vec<Action>, src: Vec<Action>) {
    let mut seen = dst
        .iter()
        .map(|a| a.action_id.clone())
        .collect::<BTreeSet<_>>();
    for action in src {
        if seen.insert(action.action_id.clone()) {
            dst.push(action);
        }
    }
}

fn prefixed_issue(source: &str, issue: &Value) -> Value {
    let mut obj = issue.as_object().cloned().unwrap_or_default();
    obj.insert("source".to_string(), Value::String(source.to_string()));
    Value::Object(obj)
}

pub(crate) fn handle_exec_summary(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let include_tasks = args_obj
        .get("include_tasks")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_jobs = args_obj
        .get("include_jobs")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let mut warnings = Vec::<Value>::new();
    let mut actions = Vec::<Action>::new();
    let mut provider_health = serde_json::Map::new();
    let mut summary = serde_json::Map::new();
    let mut critical_regressions = Vec::<Value>::new();
    let mut blockers = Vec::<Value>::new();

    if include_tasks {
        let mut task_args = serde_json::Map::new();
        if let Some(ws) = env.workspace.as_deref() {
            task_args.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
        for key in ["task", "plan", "target"] {
            if let Some(value) = args_obj.get(key) {
                task_args.insert(key.to_string(), value.clone());
            }
        }

        let tasks_resp = crate::ops::build_tasks_exec_summary(
            server,
            "tasks.exec.summary".to_string(),
            env.workspace.as_deref(),
            Value::Object(task_args),
        );
        append_actions_dedupe(&mut actions, tasks_resp.actions.clone());
        warnings.extend(tasks_resp.warnings.clone());

        if let Some(err) = tasks_resp.error.clone() {
            provider_health.insert(
                "tasks".to_string(),
                json!({ "status": "error", "error": err.to_value() }),
            );
        } else {
            provider_health.insert("tasks".to_string(), json!({ "status": "ok" }));
            summary.insert("tasks".to_string(), tasks_resp.result.clone());

            if let Some(items) = tasks_resp
                .result
                .get("critical_regressions")
                .and_then(|v| v.as_array())
            {
                critical_regressions.extend(
                    items
                        .iter()
                        .map(|issue| prefixed_issue("tasks.exec.summary", issue)),
                );
            }
            if let Some(items) = tasks_resp
                .result
                .get("exec_summary")
                .and_then(|v| v.get("radar"))
                .and_then(|v| v.get("blockers"))
                .and_then(|v| v.as_array())
            {
                blockers.extend(items.iter().map(
                    |item| json!({ "source": "tasks.exec.summary", "kind": "blocker", "value": item }),
                ));
            }
        }
    } else {
        provider_health.insert("tasks".to_string(), json!({ "status": "skipped" }));
    }

    if include_jobs {
        let mut jobs_args = serde_json::Map::new();
        if let Some(ws) = env.workspace.as_deref() {
            jobs_args.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
        if let Some(task) = args_obj.get("task") {
            jobs_args.insert("task".to_string(), task.clone());
        }
        if let Some(anchor) = args_obj.get("anchor") {
            jobs_args.insert("anchor".to_string(), anchor.clone());
        }
        jobs_args.insert(
            "view".to_string(),
            args_obj
                .get("jobs_view")
                .cloned()
                .unwrap_or_else(|| Value::String("smart".to_string())),
        );
        jobs_args.insert(
            "limit".to_string(),
            args_obj
                .get("jobs_limit")
                .cloned()
                .unwrap_or_else(|| Value::Number(20.into())),
        );
        if let Some(stall_after_s) = args_obj.get("stall_after_s") {
            jobs_args.insert("stall_after_s".to_string(), stall_after_s.clone());
        }

        let jobs_raw = server.tool_tasks_jobs_control_center(Value::Object(jobs_args));
        let jobs_resp = handler_to_op_response(&env.cmd, env.workspace.as_deref(), jobs_raw);
        append_actions_dedupe(&mut actions, jobs_resp.actions.clone());
        warnings.extend(jobs_resp.warnings.clone());

        if let Some(err) = jobs_resp.error.clone() {
            provider_health.insert(
                "jobs".to_string(),
                json!({ "status": "error", "error": err.to_value() }),
            );
        } else {
            provider_health.insert("jobs".to_string(), json!({ "status": "ok" }));
            summary.insert(
                "jobs".to_string(),
                json!({
                    "scope": jobs_resp.result.get("scope").cloned().unwrap_or(Value::Null),
                    "inbox": jobs_resp.result.get("inbox").cloned().unwrap_or(Value::Null),
                    "execution_health": jobs_resp.result.get("execution_health").cloned().unwrap_or(Value::Null),
                    "proof_health": jobs_resp.result.get("proof_health").cloned().unwrap_or(Value::Null),
                    "defaults": jobs_resp.result.get("defaults").cloned().unwrap_or(Value::Null)
                }),
            );
            if let Some(items) = jobs_resp
                .result
                .get("inbox")
                .and_then(|v| v.get("items"))
                .and_then(|v| v.as_array())
            {
                for item in items {
                    let severity = item
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_ascii_uppercase();
                    if severity == "P0" || severity == "P1" {
                        critical_regressions.push(prefixed_issue("jobs.control.center", item));
                        if let Some(job_id) = item.get("job_id").and_then(|v| v.as_str()) {
                            actions.push(Action {
                                action_id: format!("exec.summary.jobs.open::{job_id}"),
                                priority: if severity == "P0" {
                                    ActionPriority::High
                                } else {
                                    ActionPriority::Medium
                                },
                                tool: "jobs".to_string(),
                                args: json!({
                                    "workspace": env.workspace.as_deref(),
                                    "op": "open",
                                    "args": { "job": job_id },
                                    "budget_profile": "portal",
                                    "portal_view": "compact"
                                }),
                                why: "Inspect critical jobs attention item (P0/P1) and decide rotate/cancel/proof response.".to_string(),
                                risk: "Низкий".to_string(),
                            });
                        }
                    }
                    if severity == "P0" {
                        blockers.push(json!({
                            "source": "jobs.control.center",
                            "kind": "critical_attention",
                            "value": item
                        }));
                    }
                }
            }
        }
    } else {
        provider_health.insert("jobs".to_string(), json!({ "status": "skipped" }));
    }

    if !include_tasks && !include_jobs {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "at least one provider must be enabled".to_string(),
                recovery: Some(
                    "Set include_tasks=true and/or include_jobs=true (or omit both).".to_string(),
                ),
            },
        );
    }

    let now = if let Some(ws) = env.workspace.as_deref() {
        match crate::WorkspaceId::try_new(ws.to_string()) {
            Ok(workspace_id) => {
                let report = crate::ops::derive_next(server, &workspace_id);
                json!({
                    "headline": report.headline,
                    "focus": report.focus_id,
                    "state_fingerprint": report.state_fingerprint
                })
            }
            Err(_) => Value::Null,
        }
    } else {
        Value::Null
    };

    let critical_regressions_count = critical_regressions.len();
    let result = json!({
        "workspace": env.workspace.as_deref(),
        "now": now,
        "summary": Value::Object(summary),
        "critical_regressions": critical_regressions,
        "critical_regressions_count": critical_regressions_count,
        "blockers": blockers,
        "provider_health": Value::Object(provider_health),
        "source": {
            "tasks": "tasks.exec.summary",
            "jobs": "jobs.control.center"
        }
    });
    let mut resp = OpResponse::success(env.cmd.clone(), result);
    resp.warnings = warnings;
    resp.actions = actions;
    resp
}

pub(crate) fn handle_ops_summary(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

pub(crate) fn handle_cmd_list(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let mode = args_obj
        .get("mode")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "golden".to_string());
    let include_all = match mode.as_str() {
        "golden" => false,
        "all" => true,
        // Backward-compatible aliases.
        "names" => false,
        _ => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "mode must be one of: golden|all".to_string(),
                    recovery: Some("Use mode=\"golden\" (default) or mode=\"all\".".to_string()),
                },
            );
        }
    };
    let mut unknown = args_obj
        .keys()
        .filter(|k| {
            !matches!(
                k.as_str(),
                // command args
                "prefix" | "q" | "offset" | "limit" | "mode"
                // injected/envelope budget keys (must be ignored, not rejected)
                | "workspace" | "context_budget" | "max_chars"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "UNKNOWN_ARG".to_string(),
                message: format!("unknown args for system.cmd.list: {}", unknown.join(", ")),
                recovery: Some(
                    "Remove unknown args and retry. Supported args: prefix?, q?, offset?, limit?, mode?."
                        .to_string(),
                ),
            },
        );
    }
    let q = args_obj
        .get("q")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let prefix = args_obj
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let offset = args_obj.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args_obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let mut cmds = CommandRegistry::global()
        .specs()
        .iter()
        .filter(|spec| include_all || spec.tier == Tier::Gold)
        .map(|spec| spec.cmd.clone())
        .collect::<Vec<_>>();
    cmds.sort();
    cmds.dedup();
    let mut filtered = Vec::<String>::new();
    for cmd in cmds {
        if let Some(prefix) = prefix.as_deref()
            && !cmd.starts_with(prefix)
        {
            continue;
        }
        if let Some(q) = q.as_deref()
            && !cmd.to_ascii_lowercase().contains(q)
        {
            continue;
        }
        filtered.push(cmd);
    }
    let total = filtered.len();

    let page = filtered
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

pub(crate) fn handle_tutorial(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    let mut summary = "Пошаговый старт: 1) status → контекст, 2) tasks.macro.start → первая задача, 3) think.trace.sequential.step → структурный reasoning checkpoint, 4) tasks.snapshot → фокус.".to_string();
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
            "id": "sequential-checkpoint",
            "title": "Зафиксировать reasoning checkpoint",
            "tool": "think",
            "cmd": "think.trace.sequential.step",
            "purpose": "Структурно записать hypothesis→test→evidence→decision для текущего шага.",
            "action_id": "tutorial::think.trace.sequential.step"
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
                obj.insert(
                    "portal_view".to_string(),
                    Value::String("compact".to_string()),
                );
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
                obj.insert(
                    "portal_view".to_string(),
                    Value::String("compact".to_string()),
                );
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
                obj.insert(
                    "portal_view".to_string(),
                    Value::String("compact".to_string()),
                );
                Value::Object(obj)
            }
            Some("sequential-checkpoint") => {
                let mut obj = serde_json::Map::new();
                if let Some(ws) = workspace {
                    obj.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                obj.insert("op".to_string(), Value::String("call".to_string()));
                obj.insert(
                    "cmd".to_string(),
                    Value::String("think.trace.sequential.step".to_string()),
                );
                obj.insert(
                    "args".to_string(),
                    json!({
                        "thought": "Checkpoint: hypothesis/test/evidence/decision status.",
                        "thoughtNumber": 1,
                        "totalThoughts": 1,
                        "nextThoughtNeeded": false,
                        "meta": {
                            "checkpoint": "gate",
                            "hypothesis": "Current approach should pass gate.",
                            "test": "Run make check and inspect first red.",
                            "evidence": "Attach concise output snippet.",
                            "decision": "Proceed with minimal fix or stop."
                        }
                    }),
                );
                obj.insert(
                    "budget_profile".to_string(),
                    Value::String("portal".to_string()),
                );
                obj.insert(
                    "portal_view".to_string(),
                    Value::String("compact".to_string()),
                );
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

pub(crate) fn handle_migration_lookup(
    _server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={old_name:\"tasks_snapshot\"}".to_string()),
            },
        );
    };
    let raw = args_obj
        .get("old_name")
        .or_else(|| args_obj.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let old_name = normalize_migration_name(raw);
    if old_name.is_empty() {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "old_name is required".to_string(),
                recovery: Some("Provide args.old_name".to_string()),
            },
        );
    }

    let registry = CommandRegistry::global();
    let mut found: Option<&CommandSpec> = None;
    for spec in registry.specs() {
        if let Some(handler_name) = spec.handler_name.as_deref()
            && handler_name.eq_ignore_ascii_case(&old_name)
        {
            found = Some(spec);
            break;
        }
    }
    let Some(spec) = found else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "UNKNOWN_TOOL".to_string(),
                message: format!("Unknown deprecated tool name: {old_name}"),
                recovery: Some("Check docs/contracts/V1_MIGRATION.md.".to_string()),
            },
        );
    };

    let bundle = schema_bundle_for_cmd(&spec.cmd, env.workspace.as_deref()).ok();
    let mut result = json!({
        "old_name": old_name,
        "cmd": spec.cmd,
        "portal": spec.domain_tool.as_str(),
        "doc_ref": { "path": spec.doc_ref.path, "anchor": spec.doc_ref.anchor }
    });
    if let Some(bundle) = bundle
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("example_valid_call".to_string(), bundle.example_valid_call);
    }

    OpResponse::success(env.cmd.clone(), result)
}

fn normalize_migration_name(raw: &str) -> String {
    let mut name = raw.trim();
    if let Some((_, suffix)) = name.rsplit_once('/') {
        name = suffix;
    }
    if let Some((prefix, suffix)) = name.split_once('.')
        && prefix == "branchmind"
    {
        name = suffix;
    }
    name.trim().to_string()
}
