#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, BudgetCaps, BudgetPolicy, BudgetProfile, CommandSpec, ConfirmLevel,
    DocRef, Envelope, OpError, OpResponse, Safety, SchemaSource, Stability, Tier, ToolName,
};
use serde_json::{Value, json};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    specs.push(CommandSpec {
        cmd: "think.knowledge.search".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.search".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy {
            default_profile: BudgetProfile::Portal,
            portal_caps: BudgetCaps {
                max_chars: Some(6_000),
                context_budget: Some(6_000),
                limit: Some(20),
            },
            default_caps: BudgetCaps {
                max_chars: Some(20_000),
                context_budget: Some(20_000),
                limit: Some(60),
            },
            audit_caps: BudgetCaps {
                max_chars: Some(80_000),
                context_budget: Some(80_000),
                limit: Some(120),
            },
        },
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Search query (matches anchor_id/key/card_id via the knowledge key index)." },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["text"]
            }),
            example_minimal_args: json!({ "text": "core" }),
        },
        op_aliases: vec!["knowledge.search".to_string()],
        handler_name: None,
        handler: Some(handle_knowledge_search),
    });
}

fn handle_knowledge_search(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(ws) = env.workspace.as_deref() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "workspace is required".to_string(),
                recovery: Some(
                    "Call workspace op=use first (or configure default workspace).".to_string(),
                ),
            },
        );
    };
    let workspace = match crate::WorkspaceId::try_new(ws.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some("Use workspace like my-workspace".to_string()),
                },
            );
        }
    };

    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={...}".to_string()),
            },
        );
    };

    let text = args_obj
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "text must not be empty".to_string(),
                recovery: Some(
                    "Provide args={text:\"...\"} (matches anchor_id/key/card_id).".to_string(),
                ),
            },
        );
    }

    let mut warnings = Vec::<Value>::new();
    let text_owned = if text.chars().count() > 200 {
        warnings.push(crate::warning(
            "QUERY_TRUNCATED",
            "text query truncated",
            "Use a shorter query (<= 200 chars) for deterministic, bounded results.",
        ));
        Some(text.chars().take(200).collect::<String>())
    } else {
        None
    };
    let text = text_owned.as_deref().unwrap_or(text);

    let limit = args_obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
    let max_chars = args_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let found = match server.store.knowledge_keys_search(
        &workspace,
        bm_storage::KnowledgeKeysSearchRequest {
            text: text.to_string(),
            limit,
        },
    ) {
        Ok(v) => v,
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: None,
                },
            );
        }
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "STORE_ERROR".to_string(),
                    message: format!("store error: {err}"),
                    recovery: Some("Retry after initializing the workspace.".to_string()),
                },
            );
        }
    };

    let mut items = found
        .items
        .into_iter()
        .map(|row| {
            json!({
                "anchor_id": row.anchor_id,
                "key": row.key,
                "card_id": row.card_id,
                "created_at_ms": row.created_at_ms,
                "updated_at_ms": row.updated_at_ms,
            })
        })
        .collect::<Vec<_>>();

    // Prefer recency-first ordering.
    items.sort_by(|a, b| {
        let at = a.get("updated_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let bt = b.get("updated_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        bt.cmp(&at)
    });

    let mut result = json!({
        "workspace": workspace.as_str(),
        "text": text,
        "limit": limit,
        "items": items,
        "has_more": found.has_more,
    });

    crate::redact_value(&mut result, 6);

    if let Some(limit) = max_chars {
        let (limit, clamped) = crate::clamp_budget_max(limit);
        let mut truncated = false;
        let mut minimal = false;
        let _used =
            crate::ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |v| {
                let Some(items) = v.get_mut("items").and_then(|vv| vv.as_array_mut()) else {
                    return false;
                };
                if items.is_empty() {
                    return false;
                }
                items.pop();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("has_more".to_string(), Value::Bool(true));
                }
                true
            });
        warnings.extend(crate::budget_warnings(truncated, minimal, clamped));
    }

    let mut actions = Vec::<Action>::new();
    if let Some(items) = result.get("items").and_then(|v| v.as_array()) {
        for (idx, item) in items.iter().enumerate() {
            let Some(card_id) = item.get("card_id").and_then(|v| v.as_str()) else {
                continue;
            };
            actions.push(Action {
                action_id: format!("jump.open.card::{idx}"),
                priority: ActionPriority::High,
                tool: ToolName::Open.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "id": card_id,
                    "include_content": true,
                    "budget_profile": "portal",
                    "portal_view": "compact"
                }),
                why: "Открыть карточку (jump).".to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }

    let mut resp = OpResponse::success(env.cmd.clone(), result);
    resp.warnings = warnings;
    resp.actions = actions;
    resp
}
