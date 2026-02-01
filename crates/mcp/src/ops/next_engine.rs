#![forbid(unsafe_code)]

use crate::WorkspaceId;
use crate::ops::{Action, ActionPriority, ToolName};
use serde_json::json;

#[derive(Clone, Debug)]
pub(crate) struct NextEngineReport {
    pub(crate) headline: String,
    pub(crate) refs: Vec<String>,
    pub(crate) actions: Vec<Action>,
    pub(crate) state_fingerprint: String,
    pub(crate) focus_id: Option<String>,
    pub(crate) checkout: Option<String>,
}

/// NextEngine v1 (minimal viable):
/// - deterministic,
/// - low-noise,
/// - action-first.
///
/// We intentionally keep this conservative at first:
/// - show *navigation* actions (`open`, `tasks.snapshot`) before mutating actions,
/// - avoid deep heuristics (they belong in domain signals / lint engines).
pub(crate) fn derive_next(
    server: &mut crate::McpServer,
    workspace: &WorkspaceId,
) -> NextEngineReport {
    let focus_id = server.store.focus_get(workspace).ok().flatten();
    let checkout = server.store.branch_checkout_get(workspace).ok().flatten();
    let state_fingerprint = format!(
        "ws={};focus={};checkout={}",
        workspace.as_str(),
        focus_id.as_deref().unwrap_or("-"),
        checkout.as_deref().unwrap_or("-"),
    );

    let mut refs = Vec::<String>::new();
    if let Some(focus) = focus_id.as_ref() {
        refs.push(focus.clone());
    }

    let mut actions = Vec::<Action>::new();
    if let Some(focus) = focus_id.as_ref() {
        actions.push(Action {
            action_id: "next::open.focus".to_string(),
            priority: ActionPriority::High,
            tool: ToolName::Open.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "id": focus,
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Открыть текущий focus (быстрый контекст).".to_string(),
            risk: "Низкий".to_string(),
        });
        actions.push(Action {
            action_id: "next::tasks.snapshot".to_string(),
            priority: ActionPriority::Medium,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "call",
                "cmd": "tasks.snapshot",
                "args": { "view": "smart" },
                "budget_profile": "portal"
            }),
            why: "Показать snapshot (focus + next signals).".to_string(),
            risk: "Низкий".to_string(),
        });
        actions.push(Action {
            action_id: "next::think.knowledge.query".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "knowledge.query",
                "args": { "limit": 12 },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Подтянуть релевантные knowledge cards (bounded) перед решением.".to_string(),
            risk: "Низкий".to_string(),
        });
        NextEngineReport {
            headline: "Focus set: review snapshot then take the next smallest action.".to_string(),
            refs,
            actions,
            state_fingerprint,
            focus_id,
            checkout,
        }
    } else {
        actions.push(Action {
            action_id: "next::tasks.plan.create".to_string(),
            priority: ActionPriority::High,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "plan.create",
                "args": { "title": "<Plan title>" },
                "budget_profile": "default"
            }),
            why: "Создать план (золотой старт).".to_string(),
            risk: "Низкий".to_string(),
        });
        NextEngineReport {
            headline: "No focus: create a plan (or set focus) to begin.".to_string(),
            refs,
            actions,
            state_fingerprint,
            focus_id,
            checkout,
        }
    }
}
