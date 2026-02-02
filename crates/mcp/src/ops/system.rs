#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandRegistry, CommandSpec, ConfirmLevel, DocRef, Envelope, OpError,
    OpResponse, Safety, SchemaSource, Stability, Tier, ToolName, doc_ref_exists,
    schema_bundle_for_cmd,
};
use serde_json::json;

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
        legacy_tool: None,
        handler: Some(handle_schema_get),
    });

    // system.cmd.list (custom)
    specs.push(CommandSpec {
        cmd: "system.cmd.list".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Advanced,
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
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({ "prefix": "tasks." }),
        },
        op_aliases: Vec::new(),
        legacy_tool: None,
        handler: Some(handle_cmd_list),
    });

    // system.migration.lookup (custom)
    specs.push(CommandSpec {
        cmd: "system.migration.lookup".to_string(),
        domain_tool: ToolName::SystemOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#system.migration.lookup".to_string(),
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
                    "old_name": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["old_name"]
            }),
            example_minimal_args: json!({ "old_name": "tasks_snapshot" }),
        },
        op_aliases: vec!["migration.lookup".to_string()],
        legacy_tool: None,
        handler: Some(handle_migration_lookup),
    });

    // Minimal legacy system-ish tools exposed via cmd=system.<name>.
    for legacy_tool in ["storage", "init", "help", "skill", "diagnostics"] {
        let tier = match legacy_tool {
            "storage" | "diagnostics" => Tier::Internal,
            _ => Tier::Advanced,
        };
        specs.push(CommandSpec {
            cmd: format!("system.{legacy_tool}"),
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
            schema: SchemaSource::Legacy,
            op_aliases: Vec::new(),
            legacy_tool: Some(legacy_tool.to_string()),
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
    // Docs drift is a CI/maintainer concern (we keep hard guards in tests), but agents need
    // schema.get to work *even when docs anchors drift locally*. Return the schema bundle and
    // surface the drift as a warning instead of a hard error.
    let mut resp = OpResponse::success(
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
    );

    if !doc_ref_exists(&bundle.doc_ref) {
        resp.warnings.push(crate::warning(
            "DOCS_DRIFT",
            &format!(
                "doc_ref missing: {} ({})",
                bundle.doc_ref.path, bundle.doc_ref.anchor
            ),
            "Fix docs/contracts/V1_COMMANDS.md anchors or registry doc_ref (CI will enforce).",
        ));
    }

    resp
}

fn handle_cmd_list(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let args_obj = env.args.as_object().cloned().unwrap_or_default();
    let prefix = args_obj
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let offset = args_obj.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args_obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let mut cmds = CommandRegistry::global().list_cmds();
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

fn handle_migration_lookup(_server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
    let old_name = normalize_legacy_tool_name(raw);
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
        if let Some(legacy) = spec.legacy_tool.as_deref()
            && legacy.eq_ignore_ascii_case(&old_name)
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
                message: format!("Unknown legacy tool: {old_name}"),
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

fn normalize_legacy_tool_name(raw: &str) -> String {
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
