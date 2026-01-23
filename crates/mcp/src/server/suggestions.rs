#![forbid(unsafe_code)]

use serde_json::Value;
use std::collections::HashSet;

pub(super) fn advertised_tool_names(toolset: crate::Toolset) -> HashSet<String> {
    crate::tools::tool_definitions(toolset)
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<HashSet<_>>()
}

pub(super) fn escalation_toolset_for_hidden(
    hidden_targets: &[String],
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) -> Option<&'static str> {
    let mut needs_daily = false;
    let mut needs_full = false;
    for target in hidden_targets {
        if core_tools.contains(target) {
            continue;
        }
        if daily_tools.contains(target) {
            needs_daily = true;
        } else {
            needs_full = true;
        }
    }

    if needs_full {
        Some("full")
    } else if needs_daily {
        Some("daily")
    } else {
        None
    }
}

pub(super) fn sanitize_engine_calls_in_value(
    value: &mut Value,
    advertised: &HashSet<String>,
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) {
    match value {
        Value::Object(obj) => {
            for (key, child) in obj.iter_mut() {
                if key == "engine" {
                    sanitize_engine_calls_in_engine(child, advertised, core_tools, daily_tools);
                } else {
                    sanitize_engine_calls_in_value(child, advertised, core_tools, daily_tools);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                sanitize_engine_calls_in_value(child, advertised, core_tools, daily_tools);
            }
        }
        _ => {}
    }
}

fn sanitize_engine_calls_in_engine(
    engine: &mut Value,
    advertised: &HashSet<String>,
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) {
    let Some(engine_obj) = engine.as_object_mut() else {
        return;
    };
    let Some(actions) = engine_obj.get_mut("actions").and_then(|v| v.as_array_mut()) else {
        return;
    };

    for action in actions.iter_mut() {
        let Some(action_obj) = action.as_object_mut() else {
            continue;
        };
        let Some(calls) = action_obj.get_mut("calls").and_then(|v| v.as_array_mut()) else {
            continue;
        };

        let mut hidden_targets = Vec::new();
        for call in calls.iter() {
            if call.get("action").and_then(|v| v.as_str()) != Some("call_tool") {
                continue;
            }
            let target = call
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if target.is_empty() {
                continue;
            }
            if !advertised.contains(target) {
                hidden_targets.push(target.to_string());
            }
        }

        let Some(escalation_toolset) =
            escalation_toolset_for_hidden(&hidden_targets, core_tools, daily_tools)
        else {
            continue;
        };

        let already_has_disclosure = calls.iter().any(|s| {
            s.get("action").and_then(|v| v.as_str()) == Some("call_method")
                && s.get("method").and_then(|v| v.as_str()) == Some("tools/list")
        });
        if !already_has_disclosure {
            calls.insert(
                0,
                crate::suggest_method(
                    "tools/list",
                    "Reveal the next toolset tier for this engine action.",
                    "high",
                    serde_json::json!({ "toolset": escalation_toolset }),
                ),
            );
        }

        let mut seen = HashSet::new();
        calls.retain(|s| match serde_json::to_string(s) {
            Ok(key) => seen.insert(key),
            Err(_) => true,
        });
    }
}

pub(super) fn inject_smart_navigation_suggestions(
    tool: &str,
    args: &Value,
    resp_obj: &mut serde_json::Map<String, Value>,
) {
    if super::portal::is_portal_tool(tool) {
        return;
    }
    if !resp_obj
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }

    let budget_warning = super::budgets::auto_budget_escalation_allowlist(tool)
        && super::budgets::response_obj_has_budget_truncation_warning(resp_obj);
    let budget_snapshot = if budget_warning {
        super::budgets::extract_budget_snapshot_from_obj(resp_obj)
    } else {
        None
    };
    let next_cursor = super::budgets::extract_result_next_cursor(resp_obj);

    let Some(suggestions) = resp_obj
        .get_mut("suggestions")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    if !suggestions.is_empty() {
        return;
    }

    // 1) Budget friction: if the response was truncated, give a single "show more" action
    // that replays the same call with a larger budget. This keeps agents out of manual
    // max_chars guessing while preserving determinism (suggestion only; no auto-writes).
    if let Some((current_max_chars, used_chars)) = budget_snapshot
        && let Some(args_obj) = args.as_object()
    {
        let cap = super::budgets::auto_budget_escalation_cap_chars(tool);
        if current_max_chars < cap {
            let used = used_chars.unwrap_or(current_max_chars);
            let mut next_max_chars = current_max_chars
                .saturating_mul(2)
                .max(used.saturating_mul(2))
                .max(current_max_chars.saturating_add(1));
            if next_max_chars > cap {
                next_max_chars = cap;
            }

            if next_max_chars > current_max_chars {
                let mut params = args_obj.clone();
                super::budgets::apply_auto_escalated_budget(&mut params, next_max_chars);
                suggestions.push(crate::suggest_call(
                    tool,
                    "Show more (increase output budget).",
                    "high",
                    Value::Object(params),
                ));
                return;
            }
        }
    }

    // 2) "Button-like" navigation: if a result has a next_cursor, offer a single "show more"
    // pagination action (no extra parameters beyond cursor).
    if let Some(next_cursor) = next_cursor
        && let Some(args_obj) = args.as_object()
    {
        let mut params = args_obj.clone();
        params.insert(
            "cursor".to_string(),
            Value::Number(serde_json::Number::from(next_cursor)),
        );
        suggestions.push(crate::suggest_call(
            tool,
            "Show more (next page).",
            "medium",
            Value::Object(params),
        ));
    }
}

pub(super) fn portal_recovery_suggestion(
    target: &str,
    params: &Value,
    _tool: &str,
    args: &Value,
    error_code: Option<&str>,
    default_workspace: Option<&str>,
) -> Option<Value> {
    match (target, error_code) {
        ("init", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "status",
                "Auto-init workspace and show status (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_templates_list", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_templates_list",
                "List built-in templates.",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_verify", Some("CHECKPOINTS_NOT_CONFIRMED")) => {
            let mut call_params = serde_json::Map::new();

            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some(task) = params.get("task").and_then(|v| v.as_str()) {
                call_params.insert("task".to_string(), Value::String(task.to_string()));
            }
            if let Some(step_id) = params.get("step_id").and_then(|v| v.as_str()) {
                call_params.insert("step_id".to_string(), Value::String(step_id.to_string()));
            }
            if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
                call_params.insert("path".to_string(), Value::String(path.to_string()));
            }
            let checkpoints = params
                .get("checkpoints")
                .cloned()
                .unwrap_or(Value::String("gate".to_string()));
            call_params.insert("checkpoints".to_string(), checkpoints);

            maybe_omit_default_workspace(&mut call_params, default_workspace);

            Some(crate::suggest_call(
                "tasks_macro_close_step",
                "Confirm missing checkpoints + close step (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_context", Some("REVISION_MISMATCH")) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                call_params.insert(key.to_string(), Value::String(id));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_snapshot",
                "Refresh snapshot to get the current revision and retry (portal).",
                "high",
                Value::Object(call_params),
            ))
        }
        ("tasks_resume", _) | ("tasks_resume_pack", _) | ("tasks_resume_super", _) => {
            let mut call_params = serde_json::Map::new();
            if let Some(workspace) = params.get("workspace").and_then(|v| v.as_str()) {
                call_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.to_string()),
                );
            }
            if let Some(task) = params.get("task").and_then(|v| v.as_str()) {
                call_params.insert("task".to_string(), Value::String(task.to_string()));
            }
            if let Some(plan) = params.get("plan").and_then(|v| v.as_str()) {
                call_params.insert("plan".to_string(), Value::String(plan.to_string()));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            Some(crate::suggest_call(
                "tasks_snapshot",
                "Use snapshot (portal) instead of low-level resume views.",
                "medium",
                Value::Object(call_params),
            ))
        }
        _ => None,
    }
}

pub(super) fn inject_portal_recovery_for_error(
    tool: &str,
    args: &Value,
    error_code: Option<&str>,
    error_message: Option<&str>,
    suggestions: &mut Vec<Value>,
    default_workspace: Option<&str>,
) {
    // Recovery UX applies to the whole server surface, but keep it conservative:
    // - Only run when there are no suggestions at all.
    // - Only inject for the tasks subsystem (daily DX driver), to avoid surprising
    //   behavior in unrelated tool families.
    if !tool.starts_with("tasks_") {
        return;
    }
    if !suggestions.is_empty() {
        return;
    }

    let workspace = args
        .as_object()
        .and_then(|obj| obj.get("workspace"))
        .and_then(|v| v.as_str());

    match error_code {
        Some("UNKNOWN_ID") => {
            // Keep the agent productive without forcing a full toolset disclosure.
            // - If a step selector was wrong, show a snapshot for the current target (if any).
            // - If a target id was wrong, show snapshot for focus (drop explicit target), plus a
            //   safe portal fallback to re-establish focus.
            // - If focus itself is broken, suggest starting a new task (portal).
            let msg = error_message.unwrap_or("");
            let is_step_like = msg.contains("Step not found")
                || msg.contains("Parent step not found")
                || msg.contains("Task node not found");

            if is_step_like {
                let mut call_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                    call_params.insert(key.to_string(), Value::String(id));
                }
                maybe_omit_default_workspace(&mut call_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_snapshot",
                    "Open snapshot to confirm ids and selectors (portal).",
                    "high",
                    Value::Object(call_params),
                ));
                return;
            }

            let has_explicit_target = extract_task_or_plan_from_args(args).is_some();
            if has_explicit_target {
                let mut call_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                // Intentionally omit task/plan: a stale id should not keep failing. Prefer focus.
                maybe_omit_default_workspace(&mut call_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_snapshot",
                    "Open snapshot (portal) to confirm focus and valid ids.",
                    "high",
                    Value::Object(call_params),
                ));

                let mut start_params = serde_json::Map::new();
                if let Some(ws) = workspace {
                    start_params.insert("workspace".to_string(), Value::String(ws.to_string()));
                }
                start_params.insert(
                    "task_title".to_string(),
                    Value::String("New task".to_string()),
                );
                maybe_omit_default_workspace(&mut start_params, default_workspace);
                suggestions.push(crate::suggest_call(
                    "tasks_macro_start",
                    "If focus is missing, restore it by starting a new task (portal).",
                    "medium",
                    Value::Object(start_params),
                ));
                return;
            }

            let mut start_params = serde_json::Map::new();
            if let Some(ws) = workspace {
                start_params.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            start_params.insert(
                "task_title".to_string(),
                Value::String("New task".to_string()),
            );
            maybe_omit_default_workspace(&mut start_params, default_workspace);
            suggestions.push(crate::suggest_call(
                "tasks_macro_start",
                "Restore focus by starting a new task (portal).",
                "high",
                Value::Object(start_params),
            ));
        }
        Some("REVISION_MISMATCH") => {
            // Fail-safe: if an implementation forgets to include a refresh hint, provide one.
            let mut call_params = serde_json::Map::new();
            if let Some(ws) = workspace {
                call_params.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            if let Some((key, id)) = extract_task_or_plan_from_args(args) {
                call_params.insert(key.to_string(), Value::String(id));
            }
            maybe_omit_default_workspace(&mut call_params, default_workspace);
            suggestions.push(crate::suggest_call(
                "tasks_snapshot",
                "Refresh snapshot to get the current revision (portal).",
                "high",
                Value::Object(call_params),
            ));
        }
        _ => {}
    }
}

fn maybe_omit_default_workspace(
    params: &mut serde_json::Map<String, Value>,
    default_workspace: Option<&str>,
) {
    let Some(default_workspace) = default_workspace else {
        return;
    };
    if params
        .get("workspace")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v == default_workspace)
    {
        params.remove("workspace");
    }
}

fn extract_task_or_plan_from_args(args: &Value) -> Option<(&'static str, String)> {
    let obj = args.as_object()?;
    if let Some(task) = obj.get("task").and_then(|v| v.as_str()) {
        return Some(("task", task.to_string()));
    }
    if let Some(plan) = obj.get("plan").and_then(|v| v.as_str()) {
        return Some(("plan", plan.to_string()));
    }
    None
}
