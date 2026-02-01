#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, CommandRegistry};
use serde_json::{Value, json};

pub(super) fn rewrite_actions_for_toolset(
    toolset: crate::Toolset,
    error_code: Option<&str>,
    actions: &mut Vec<Value>,
    default_workspace: Option<&str>,
) {
    if actions.is_empty() {
        return;
    }

    let registry = CommandRegistry::global();

    let mut rebuilt = Vec::<Value>::with_capacity(actions.len() + 1);

    for action in actions.iter() {
        let Some(obj) = action.as_object() else {
            rebuilt.push(action.clone());
            continue;
        };
        let tool = obj.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        let Some(args) = obj.get("args").and_then(|v| v.as_object()) else {
            rebuilt.push(action.clone());
            continue;
        };

        let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("");

        // Hygiene: v1 envelope keeps workspace at the top level; legacy suggestions sometimes
        // duplicate it in args.workspace. Strip the duplicate to keep actions copy/paste-ready.
        let mut cleaned_action = action.clone();
        if let Some(action_obj) = cleaned_action.as_object_mut()
            && let Some(env) = action_obj.get_mut("args").and_then(|v| v.as_object_mut())
        {
            strip_inner_workspace(env);
        }

        if toolset != crate::Toolset::Full
            && tool == "tasks"
            && cmd == "tasks.verify"
            && error_code == Some("CHECKPOINTS_NOT_CONFIRMED")
            && let Some(portal_action) = portalize_tasks_verify_as_macro_close_step(
                obj.get("why").and_then(|v| v.as_str()),
                obj.get("risk").and_then(|v| v.as_str()),
                args,
                default_workspace,
                registry,
            )
        {
            rebuilt.push(portal_action.to_json());
            continue;
        }

        rebuilt.push(cleaned_action);
    }

    rebuilt.sort_by_key(action_sort_key);
    actions.clear();
    actions.extend(rebuilt);
}

fn portalize_tasks_verify_as_macro_close_step(
    why: Option<&str>,
    risk: Option<&str>,
    verify_env: &serde_json::Map<String, Value>,
    default_workspace: Option<&str>,
    registry: &CommandRegistry,
) -> Option<Action> {
    let inner = verify_env.get("args").and_then(|v| v.as_object())?;

    // Map the "tasks.verify" recovery into a portal macro close that will guide the agent
    // through checkpoint confirmation and step completion.
    let mut macro_args = serde_json::Map::new();
    if let Some(task) = inner.get("task").and_then(|v| v.as_str()) {
        macro_args.insert("task".to_string(), Value::String(task.to_string()));
    }
    if let Some(step_id) = inner.get("step_id").and_then(|v| v.as_str()) {
        macro_args.insert("step_id".to_string(), Value::String(step_id.to_string()));
    }
    if let Some(path) = inner.get("path").and_then(|v| v.as_str()) {
        macro_args.insert("path".to_string(), Value::String(path.to_string()));
    }
    if let Some(checkpoints) = inner.get("checkpoints") {
        macro_args.insert("checkpoints".to_string(), checkpoints.clone());
    }

    // Preserve the outer envelope knobs when possible.
    let mut env = serde_json::Map::new();
    if let Some(ws) = verify_env
        .get("workspace")
        .and_then(|v| v.as_str())
        .or_else(|| inner.get("workspace").and_then(|v| v.as_str()))
        && default_workspace != Some(ws)
    {
        env.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    env.insert("op".to_string(), json!("call"));
    env.insert("cmd".to_string(), json!("tasks.macro.close.step"));
    env.insert("args".to_string(), Value::Object(macro_args));

    if let Some(profile) = verify_env.get("budget_profile").and_then(|v| v.as_str()) {
        env.insert(
            "budget_profile".to_string(),
            Value::String(profile.to_string()),
        );
    }
    if let Some(view) = verify_env.get("view").and_then(|v| v.as_str()) {
        env.insert("view".to_string(), Value::String(view.to_string()));
    }

    // If the macro cmd doesn't exist (registry drift), fail closed and keep the original action.
    registry.find_by_cmd("tasks.macro.close.step")?;

    Some(Action {
        action_id: "recover.portal.macro_close_step".to_string(),
        priority: ActionPriority::High,
        tool: "tasks".to_string(),
        args: Value::Object(env),
        why: why
            .unwrap_or("Close step via portal macro (checkpoint-aware).")
            .to_string(),
        risk: risk.unwrap_or("Низкий").to_string(),
    })
}

fn strip_inner_workspace(env: &mut serde_json::Map<String, Value>) {
    let outer_ws = env
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let Some(inner) = env.get_mut("args").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let inner_ws = inner.get("workspace").and_then(|v| v.as_str());
    if let Some(outer_ws) = outer_ws.as_deref()
        && inner_ws == Some(outer_ws)
    {
        inner.remove("workspace");
    }
}

fn action_sort_key(action: &Value) -> (u8, String) {
    let Some(obj) = action.as_object() else {
        return (2, String::new());
    };
    let rank = match obj
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "high" => 0,
        "low" => 2,
        _ => 1,
    };
    let id = obj
        .get("action_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (rank, id)
}
