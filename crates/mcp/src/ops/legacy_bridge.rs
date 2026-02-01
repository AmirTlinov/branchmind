#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, BudgetProfile, CommandRegistry, OpError, OpResponse, ToolName,
};
use serde_json::{Value, json};

pub(crate) fn legacy_to_op_response(
    cmd: &str,
    workspace: Option<&str>,
    legacy_resp: Value,
) -> OpResponse {
    let success = legacy_resp
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let warnings = legacy_resp
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if success {
        let result = legacy_resp.get("result").cloned().unwrap_or(json!({}));
        // v1: legacy tools sometimes emit suggestions even on success (e.g. transcripts_*).
        // Convert those suggestions into portal-first `actions[]` to preserve the UX
        // while keeping `suggestions[]` empty at the public surface.
        let mut actions = Vec::<Action>::new();
        if let Some(suggestions) = legacy_resp.get("suggestions").and_then(|v| v.as_array()) {
            actions.extend(legacy_suggestions_to_actions(
                CommandRegistry::global(),
                workspace,
                suggestions,
                None,
            ));
        }
        return OpResponse {
            intent: cmd.to_string(),
            result,
            refs: Vec::new(),
            warnings,
            actions,
            error: None,
        };
    }

    let legacy_error = legacy_resp.get("error").cloned().unwrap_or(Value::Null);
    let code = legacy_error
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("INTERNAL_ERROR")
        .trim()
        .to_string();
    // v1 strategy: preserve domain-specific codes. This keeps typed errors stable across
    // the legacy → portal bridge (REVISION_MISMATCH, REASONING_REQUIRED, etc.).
    let mapped_code = if code.is_empty() {
        "INTERNAL_ERROR".to_string()
    } else {
        code
    };
    let message = legacy_error
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("error")
        .to_string();
    let recovery = legacy_error
        .get("recovery")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut actions = Vec::<Action>::new();
    if let Some(suggestions) = legacy_resp.get("suggestions").and_then(|v| v.as_array()) {
        actions.extend(legacy_suggestions_to_actions(
            CommandRegistry::global(),
            workspace,
            suggestions,
            Some(mapped_code.as_str()),
        ));
    }

    OpResponse {
        intent: cmd.to_string(),
        result: json!({}),
        refs: Vec::new(),
        warnings,
        actions,
        error: Some(OpError {
            code: mapped_code,
            message,
            recovery,
        }),
    }
}

fn legacy_suggestions_to_actions(
    registry: &CommandRegistry,
    workspace: Option<&str>,
    suggestions: &[Value],
    error_code: Option<&str>,
) -> Vec<Action> {
    let mut out = Vec::<Action>::new();
    for (idx, suggestion) in suggestions.iter().enumerate() {
        let Some(obj) = suggestion.as_object() else {
            continue;
        };
        let action = obj.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if action != "call_tool" {
            continue;
        }
        let Some(target) = obj.get("target").and_then(|v| v.as_str()) else {
            continue;
        };
        let target = match (target, error_code) {
            // Portal-first UX: replace low-level checkpoint confirmation suggestion with the
            // safe macro that both confirms checkpoints and closes the step in one flow.
            ("tasks_verify", Some("CHECKPOINTS_NOT_CONFIRMED")) => "tasks_macro_close_step",
            _ => target,
        };
        let why = obj
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Follow-up")
            .to_string();
        let priority = parse_legacy_priority(
            obj.get("priority")
                .and_then(|v| v.as_str())
                .unwrap_or("medium"),
        );
        let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));

        if target == "status" {
            let mut args = serde_json::Map::new();
            if let Some(ws) = workspace {
                args.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            args.insert(
                "budget_profile".to_string(),
                Value::String(BudgetProfile::Portal.as_str().to_string()),
            );
            args.insert("view".to_string(), Value::String("compact".to_string()));
            out.push(Action {
                action_id: format!("recover.legacy.status::{idx}"),
                priority,
                tool: ToolName::Status.as_str().to_string(),
                args: Value::Object(args),
                why,
                risk: "Низкий".to_string(),
            });
            continue;
        }

        // Most legacy suggestions reference legacy tool names; map them via registry.
        if let Some(spec) = registry.find_by_legacy_tool(target) {
            let mut env = serde_json::Map::new();
            if let Some(ws) = workspace {
                env.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            env.insert("op".to_string(), Value::String("call".to_string()));
            env.insert("cmd".to_string(), Value::String(spec.cmd.clone()));
            env.insert("args".to_string(), params);
            env.insert(
                "budget_profile".to_string(),
                Value::String(spec.budget.default_profile.as_str().to_string()),
            );
            env.insert("view".to_string(), Value::String("compact".to_string()));

            out.push(Action {
                action_id: format!(
                    "recover.legacy.call.{}::{idx}",
                    sanitize_action_id_segment(target)
                ),
                priority,
                tool: spec.domain_tool.as_str().to_string(),
                args: Value::Object(env),
                why,
                risk: "Низкий".to_string(),
            });
            continue;
        }

        // Fallback: recommend migration lookup (even though v1 surface doesn't accept old names).
        out.push(Action {
            action_id: format!(
                "recover.legacy.migration.lookup.{}::{idx}",
                sanitize_action_id_segment(target)
            ),
            priority: ActionPriority::High,
            tool: "system".to_string(),
            args: json!({
                "op": "migration.lookup",
                "args": { "old_name": target },
                "budget_profile": "portal"
            }),
            why: format!("Найти v1 cmd для legacy tool {target}."),
            risk: "Низкий".to_string(),
        });
    }
    out
}

fn parse_legacy_priority(raw: &str) -> ActionPriority {
    match raw.trim().to_ascii_lowercase().as_str() {
        "high" => ActionPriority::High,
        "low" => ActionPriority::Low,
        _ => ActionPriority::Medium,
    }
}

fn sanitize_action_id_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}
