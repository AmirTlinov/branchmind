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
        // Copy/paste-safe fallback: no placeholders. The user/agent can rename later.
        env.insert("args".to_string(), json!({ "task_title": "Recovery task" }));
        env.insert("budget_profile".to_string(), json!("portal"));
        env.insert("view".to_string(), json!("compact"));

        resp.actions.push(Action {
            action_id: start_action_id,
            priority: ActionPriority::Medium,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: serde_json::Value::Object(env),
            why: "Создать новый TASK (fallback), если целевой id больше не существует.".to_string(),
            risk: "Создаст новый TASK (fallback) с универсальным названием.".to_string(),
        });
    }
}

/// Attach portal-first recovery actions for INVALID_INPUT so BM-L1 (lines) mode remains
/// copy/paste-friendly and avoids placeholder-heavy schema examples.
pub(crate) fn append_invalid_input_actions(
    resp: &mut OpResponse,
    cmd: &str,
    workspace: Option<&str>,
) {
    let mut existing_cmds = BTreeSet::<String>::new();
    for action in resp.actions.iter() {
        if let Some(cmd) = action
            .args
            .as_object()
            .and_then(|obj| obj.get("cmd"))
            .and_then(|v| v.as_str())
        {
            existing_cmds.insert(cmd.to_string());
        }
    }

    let mut seen = BTreeSet::<String>::new();
    for a in resp.actions.iter() {
        seen.insert(a.action_id.clone());
    }

    // Tasks: re-establish the "compass" (snapshot → NextEngine actions).
    if cmd.starts_with("tasks.") {
        // If the handler already provided a macro_start recovery (the canonical no-focus path),
        // do not add extra command lines — keep BM-L1 errors tag-light.
        if existing_cmds.contains("tasks.macro.start") {
            return;
        }
        let action_id = "recover.portal.tasks.snapshot".to_string();
        if seen.insert(action_id.clone()) {
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
                action_id,
                priority: ActionPriority::High,
                tool: ToolName::TasksOps.as_str().to_string(),
                args: serde_json::Value::Object(env),
                why:
                    "Открыть snapshot (компас) и получить следующий шаг без ручного поиска команд."
                        .to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }

    // Jobs: show the job radar (safe default when required args are missing).
    if cmd.starts_with("jobs.") {
        if existing_cmds.contains("jobs.radar") || existing_cmds.contains("jobs.list") {
            return;
        }
        let action_id = "recover.portal.jobs.radar".to_string();
        if seen.insert(action_id.clone()) {
            let mut env = serde_json::Map::new();
            if let Some(ws) = workspace {
                env.insert(
                    "workspace".to_string(),
                    serde_json::Value::String(ws.to_string()),
                );
            }
            env.insert("op".to_string(), json!("call"));
            env.insert("cmd".to_string(), json!("jobs.radar"));
            env.insert("args".to_string(), json!({}));
            env.insert("budget_profile".to_string(), json!("portal"));
            env.insert("view".to_string(), json!("compact"));

            resp.actions.push(Action {
                action_id,
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: serde_json::Value::Object(env),
                why: "Показать jobs.radar (подобрать job id и следующий шаг без placeholder args)."
                    .to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }
}
