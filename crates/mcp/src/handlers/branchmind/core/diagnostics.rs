#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_diagnostics(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let checkout = match self.store.branch_checkout_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let focus = match self.store.focus_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut issues = Vec::new();
        let mut suggestions = Vec::new();

        if !workspace_exists {
            issues.push(json!({
                "severity": "error",
                "code": "WORKSPACE_MISSING",
                "message": "workspace is not initialized",
                "recovery": "Run init to bootstrap workspace storage."
            }));
            suggestions.push(suggest_call(
                "init",
                "Initialize the workspace and bootstrap a default branch.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            ));
        }

        if checkout.is_none() {
            issues.push(json!({
                "severity": "warning",
                "code": "NO_CHECKOUT",
                "message": "no checkout branch configured",
                "recovery": "Create a branch or checkout an existing branch."
            }));
            suggestions.push(suggest_call(
                "branch_list",
                "List known branches for this workspace.",
                "medium",
                json!({ "workspace": workspace.as_str() }),
            ));
        }

        let target_raw = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                args_obj
                    .get("task")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                args_obj
                    .get("plan")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| focus.clone());

        let mut target_info = Value::Null;
        let mut context_health = Value::Null;
        if let Some(target_id) = target_raw {
            let kind = match parse_plan_or_task_kind(&target_id) {
                Some(v) => v,
                None => return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-"),
            };
            let mut lint_args = serde_json::Map::new();
            lint_args.insert(
                "workspace".to_string(),
                Value::String(workspace.as_str().to_string()),
            );
            match kind {
                TaskKind::Plan => {
                    lint_args.insert("plan".to_string(), Value::String(target_id.clone()));
                }
                TaskKind::Task => {
                    lint_args.insert("task".to_string(), Value::String(target_id.clone()));
                }
            }
            let lint = self.tool_tasks_lint(Value::Object(lint_args));
            if !lint
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return lint;
            }
            if let Some(result) = lint.get("result") {
                if let Some(target) = result.get("target") {
                    target_info = target.clone();
                }
                if let Some(health) = result.get("context_health") {
                    context_health = health.clone();
                }
                if let Some(list) = result.get("issues").and_then(|v| v.as_array()) {
                    issues.extend(list.clone());
                }
            }
        } else {
            issues.push(json!({
                "severity": "warning",
                "code": "NO_TARGET",
                "message": "no task/plan target or focus set",
                "recovery": "Use tasks_context to list items, then set focus via tasks_focus_set."
            }));
            suggestions.push(suggest_call(
                "tasks_context",
                "List plans and tasks for this workspace to choose a focus target.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            ));
        }

        let (errors, warnings) = issues.iter().fold((0, 0), |acc, item| {
            match item.get("severity").and_then(|v| v.as_str()) {
                Some("error") => (acc.0 + 1, acc.1),
                Some("warning") => (acc.0, acc.1 + 1),
                _ => acc,
            }
        });

        let golden_path = json!([
            {
                "tool": "macro_branch_note",
                "purpose": "start an initiative branch + seed a first note"
            },
            {
                "tool": "tasks_macro_start",
                "purpose": "create a task with steps and open a resume capsule"
            },
            {
                "tool": "tasks_snapshot",
                "purpose": "refresh unified snapshot (tasks + reasoning + diff)"
            }
        ]);

        let mut result = json!({
            "workspace": workspace.as_str(),
            "checkout": checkout,
            "focus": focus,
            "target": target_info,
            "summary": {
                "errors": errors,
                "warnings": warnings,
                "total": errors + warnings
            },
            "issues": issues,
            "context_health": context_health,
            "golden_path": golden_path
        });

        let mut warnings_out = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["context_health"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["issues"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["golden_path"]);
                    }
                    changed
                });

            warnings_out = budget_warnings(truncated, minimal, clamped);
        }

        if warnings_out.is_empty() {
            ai_ok_with("diagnostics", result, suggestions)
        } else {
            ai_ok_with_warnings("diagnostics", result, warnings_out, suggestions)
        }
    }
}
