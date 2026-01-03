#![forbid(unsafe_code)]

use super::ai::{ai_error, ai_error_with, format_store_error, suggest_call};
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_storage::SqliteStore;
use serde_json::{Value, json};

pub(crate) fn parse_kind(kind: Option<&str>, has_parent: bool) -> TaskKind {
    match kind {
        Some("task") => TaskKind::Task,
        Some("plan") => TaskKind::Plan,
        _ => {
            if has_parent {
                TaskKind::Task
            } else {
                TaskKind::Plan
            }
        }
    }
}

pub(crate) fn parse_plan_or_task_kind(id: &str) -> Option<TaskKind> {
    if id.starts_with("PLAN-") {
        Some(TaskKind::Plan)
    } else if id.starts_with("TASK-") {
        Some(TaskKind::Task)
    } else {
        None
    }
}

pub(crate) fn normalize_target_map(
    tool: &str,
    args: &mut serde_json::Map<String, Value>,
) -> Result<(), Value> {
    if !args.contains_key("target") {
        return Ok(());
    }
    let target_value = args.get("target").cloned().unwrap_or(Value::Null);
    if target_value.is_null() {
        return Err(ai_error(
            "INVALID_INPUT",
            "target: expected string or {id, kind}; fix: target={\"id\":\"TASK-001\"}",
        ));
    }
    let (target_id, target_kind) = parse_target_ref(&target_value)?;

    if tool.starts_with("tasks_") {
        if args.contains_key("task") || args.contains_key("plan") {
            return Err(ai_error(
                "INVALID_INPUT",
                "target: expected single target; fix: provide target OR task/plan (not both)",
            ));
        }
        match target_kind {
            TaskKind::Plan => {
                args.insert("plan".to_string(), Value::String(target_id));
            }
            TaskKind::Task => {
                args.insert("task".to_string(), Value::String(target_id));
            }
        }
        args.remove("target");
        return Ok(());
    }

    // BranchMind tools use `target` as the canonical scope reference.
    // (Tool names are intentionally unprefixed; the MCP server name already namespaces them.)
    if tool != "storage" {
        args.insert("target".to_string(), Value::String(target_id));
        return Ok(());
    }

    Ok(())
}

fn parse_target_ref(value: &Value) -> Result<(String, TaskKind), Value> {
    if let Some(id) = value.as_str() {
        return parse_target_id_with_kind(id, None, "target");
    }
    let Some(obj) = value.as_object() else {
        return Err(ai_error(
            "INVALID_INPUT",
            "target: expected string or {id, kind}; fix: target={\"id\":\"TASK-001\"}",
        ));
    };
    let id = match obj.get("id").and_then(|v| v.as_str()) {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            return Err(ai_error(
                "INVALID_INPUT",
                "target.id: expected string; fix: target={\"id\":\"TASK-001\"}",
            ));
        }
    };
    let kind = obj.get("kind").and_then(|v| v.as_str());
    parse_target_id_with_kind(id, kind, "target")
}

fn parse_target_id_with_kind(
    id: &str,
    kind: Option<&str>,
    field: &str,
) -> Result<(String, TaskKind), Value> {
    let inferred = parse_plan_or_task_kind(id);
    let resolved = match (inferred, kind) {
        (Some(TaskKind::Plan), Some("plan")) => TaskKind::Plan,
        (Some(TaskKind::Task), Some("task")) => TaskKind::Task,
        (Some(kind), None) => kind,
        (None, Some("plan")) => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}: expected id starting with PLAN-; fix: target={{\"id\":\"PLAN-001\"}}"
                ),
            ));
        }
        (None, Some("task")) => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}: expected id starting with TASK-; fix: target={{\"id\":\"TASK-001\"}}"
                ),
            ));
        }
        (None, Some(_)) => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}.kind: expected 'plan' or 'task'; fix: target={{\"id\":\"TASK-001\",\"kind\":\"task\"}}"
                ),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}.kind: expected match with id prefix; fix: target={{\"id\":\"{id}\",\"kind\":\"{}\"}}",
                    if id.starts_with("PLAN-") {
                        "plan"
                    } else {
                        "task"
                    }
                ),
            ));
        }
        (None, None) => {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "{field}: expected id starting with PLAN-/TASK-; fix: target={{\"id\":\"TASK-001\"}}"
                ),
            ));
        }
    };

    Ok((id.to_string(), resolved))
}

pub(crate) fn resolve_target_id(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    args: &serde_json::Map<String, Value>,
) -> Result<(String, TaskKind, Option<String>), Value> {
    let task = args.get("task").and_then(|v| v.as_str());
    let plan = args.get("plan").and_then(|v| v.as_str());
    if task.is_some() && plan.is_some() {
        return Err(ai_error("INVALID_INPUT", "provide task or plan, not both"));
    }

    let focus = match store.focus_get(workspace) {
        Ok(v) => v,
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let target_id = task
        .map(|v| v.to_string())
        .or_else(|| plan.map(|v| v.to_string()))
        .or_else(|| focus.clone());
    let Some(target_id) = target_id else {
        // Portal-first recovery UX: avoid forcing agents into the full toolset when a workspace
        // is simply empty or focus has not been established yet.
        //
        // If tasks exist, suggest snapshots for the first couple of tasks so the agent can
        // choose a target without immediately expanding into `tasks_context`.
        // If the workspace is empty, suggest starting a first task via the portal macro.
        let tasks_count = match store.count_tasks(workspace) {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        if tasks_count <= 0 {
            return Err(ai_error_with(
                "INVALID_INPUT",
                "No target: provide task or plan, or set focus",
                Some("Workspace has no tasks yet. Create a first task via tasks_macro_start."),
                vec![suggest_call(
                    "tasks_macro_start",
                    "Create a first task and set focus (portal).",
                    "high",
                    json!({ "workspace": workspace.as_str(), "task_title": "First task" }),
                )],
            ));
        }

        let tasks = match store.list_tasks(workspace, 2, 0) {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        if tasks.is_empty() {
            return Err(ai_error_with(
                "INVALID_INPUT",
                "No target: provide task or plan, or set focus",
                Some(
                    "No focus is set, but the workspace has tasks. Use tasks_snapshot for a known id, or create a new task via tasks_macro_start.",
                ),
                vec![suggest_call(
                    "tasks_context",
                    "List plans and tasks for this workspace to choose a focus target.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ));
        }

        let mut suggestions = Vec::new();
        for (idx, row) in tasks.iter().enumerate() {
            let priority = if idx == 0 { "high" } else { "medium" };
            suggestions.push(suggest_call(
                "tasks_snapshot",
                "Open a snapshot for this task to confirm the right target.",
                priority,
                json!({ "workspace": workspace.as_str(), "task": row.id.as_str() }),
            ));
        }

        let preview = tasks
            .iter()
            .map(|row| format!("{} \"{}\"", row.id.as_str(), row.title.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let more = if tasks_count > tasks.len() as i64 {
            format!(" (+{})", tasks_count - tasks.len() as i64)
        } else {
            String::new()
        };

        return Err(ai_error_with(
            "INVALID_INPUT",
            "No target: provide task or plan, or set focus",
            Some(&format!(
                "No focus is set. Try a tasks_snapshot for a known task: {preview}{more}."
            )),
            suggestions,
        ));
    };

    let kind = if target_id.starts_with("PLAN-") {
        TaskKind::Plan
    } else if target_id.starts_with("TASK-") {
        TaskKind::Task
    } else {
        return Err(ai_error(
            "INVALID_INPUT",
            "task must start with PLAN- or TASK-",
        ));
    };

    Ok((target_id, kind, focus))
}

pub(crate) fn restore_focus_for_explicit_target(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    explicit_target: Option<&str>,
    current_focus: Option<String>,
) -> Result<(Option<String>, Option<String>, bool), Value> {
    let Some(explicit) = explicit_target else {
        return Ok((current_focus, None, false));
    };
    if current_focus.as_deref() == Some(explicit) {
        return Ok((current_focus, None, false));
    }
    if let Err(err) = store.focus_set(workspace, explicit) {
        return Err(ai_error("STORE_ERROR", &format_store_error(err)));
    }
    Ok((Some(explicit.to_string()), current_focus, true))
}
