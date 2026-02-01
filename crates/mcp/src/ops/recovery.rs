#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use serde_json::json;

use crate::ops::{Action, ActionPriority, OpResponse, ToolName};

/// Attach portal-first recovery actions for typed errors that otherwise leave the agent without
/// a clear next move.
///
/// v1 goal: even in BM-L1 (lines) mode, errors should usually have a "next command" line.
pub(crate) fn append_unknown_id_actions(resp: &mut OpResponse, cmd: &str, workspace: Option<&str>) {
    if !cmd.starts_with("tasks.") {
        return;
    }

    let mut seen = BTreeSet::<String>::new();
    for a in resp.actions.iter() {
        seen.insert(a.action_id.clone());
    }

    // Primary recovery: re-run a snapshot without a stale/unknown id to re-establish focus
    // and present the safe next action rail.
    let snapshot_action_id = "recover.portal.snapshot".to_string();
    if seen.insert(snapshot_action_id.clone()) {
        let mut env = serde_json::Map::new();
        if let Some(ws) = workspace {
            env.insert(
                "workspace".to_string(),
                serde_json::Value::String(ws.to_string()),
            );
        }
        env.insert("op".to_string(), json!("call"));
        env.insert("cmd".to_string(), json!("tasks.snapshot"));
        env.insert("args".to_string(), json!({}));
        env.insert("budget_profile".to_string(), json!("portal"));
        env.insert("view".to_string(), json!("compact"));

        resp.actions.push(Action {
            action_id: snapshot_action_id,
            priority: ActionPriority::High,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: serde_json::Value::Object(env),
            why: "Обновить snapshot без неизвестного id (восстановить фокус и следующий шаг)."
                .to_string(),
            risk: "Низкий".to_string(),
        });
    }

    // Secondary recovery: offer a safe, portal-grade way to create a new task when the
    // referenced id is gone/invalid.
    let start_action_id = "recover.portal.macro.start".to_string();
    if seen.insert(start_action_id.clone()) {
        let mut env = serde_json::Map::new();
        if let Some(ws) = workspace {
            env.insert(
                "workspace".to_string(),
                serde_json::Value::String(ws.to_string()),
            );
        }
        env.insert("op".to_string(), json!("call"));
        env.insert("cmd".to_string(), json!("tasks.macro.start"));
        env.insert("args".to_string(), json!({ "task_title": "<task_title>" }));
        env.insert("budget_profile".to_string(), json!("portal"));
        env.insert("view".to_string(), json!("compact"));

        resp.actions.push(Action {
            action_id: start_action_id,
            priority: ActionPriority::Medium,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: serde_json::Value::Object(env),
            why: "Создать новый TASK (fallback), если целевой id больше не существует.".to_string(),
            risk: "Может создать TASK с placeholder названием, если запустить без правки."
                .to_string(),
        });
    }
}
