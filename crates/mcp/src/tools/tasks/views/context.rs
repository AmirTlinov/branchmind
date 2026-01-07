#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_context(&mut self, args: Value) -> Value {
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
        let plans_limit = args_obj
            .get("plans_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let plans_cursor = args_obj
            .get("plans_cursor")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);
        let tasks_limit = args_obj
            .get("tasks_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let tasks_cursor = args_obj
            .get("tasks_cursor")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        let plans_total = match self.store.count_plans(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tasks_total = match self.store.count_tasks(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let plans = match self.store.list_plans(&workspace, plans_limit, plans_cursor) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tasks = match self.store.list_tasks(&workspace, tasks_limit, tasks_cursor) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let plans_out = plans
            .into_iter()
            .map(|p| {
                let checklist = self.store.plan_checklist_get(&workspace, &p.id).ok();
                let progress = checklist
                    .as_ref()
                    .map(|c| format!("{}/{}", c.current, c.steps.len()))
                    .unwrap_or_else(|| "0/0".to_string());
                json!({
                    "id": p.id,
                    "qualified_id": format!("{}:{}", workspace.as_str(), p.id),
                    "kind": "plan",
                    "title": p.title,
                    "revision": p.revision,
                    "contract_versions_count": 0,
                    "created_at_ms": p.created_at_ms,
                    "updated_at_ms": p.updated_at_ms,
                    "criteria_confirmed": p.criteria_confirmed,
                    "tests_confirmed": p.tests_confirmed,
                    "criteria_auto_confirmed": p.criteria_auto_confirmed,
                    "tests_auto_confirmed": p.tests_auto_confirmed,
                    "security_confirmed": p.security_confirmed,
                    "perf_confirmed": p.perf_confirmed,
                    "docs_confirmed": p.docs_confirmed,
                    "plan_progress": progress
                })
            })
            .collect::<Vec<_>>();

        let mut by_status = std::collections::BTreeMap::new();
        let tasks_out = tasks
            .into_iter()
            .map(|t| {
                *by_status.entry(t.status.clone()).or_insert(0usize) += 1;
                let summary = self.store.task_steps_summary(&workspace, &t.id).ok();
                let steps_count = summary.as_ref().map(|s| s.total_steps).unwrap_or(0);
                let progress = if steps_count == 0 {
                    0
                } else {
                    ((summary.as_ref().map(|s| s.completed_steps).unwrap_or(0) as f64)
                        / (steps_count as f64)
                        * 100.0)
                        .round() as i64
                };
                json!({
                    "id": t.id,
                    "qualified_id": format!("{}:{}", workspace.as_str(), t.id),
                    "kind": "task",
                    "title": t.title,
                    "revision": t.revision,
                    "status": t.status,
                    "status_code": t.status,
                    "created_at_ms": t.created_at_ms,
                    "updated_at_ms": t.updated_at_ms,
                    "progress": progress,
                    "criteria_confirmed": t.criteria_confirmed,
                    "tests_confirmed": t.tests_confirmed,
                    "criteria_auto_confirmed": t.criteria_auto_confirmed,
                    "tests_auto_confirmed": t.tests_auto_confirmed,
                    "security_confirmed": t.security_confirmed,
                    "perf_confirmed": t.perf_confirmed,
                    "docs_confirmed": t.docs_confirmed,
                    "parent": t.parent_plan_id,
                    "steps_count": steps_count
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "counts": {
                "plans": plans_total,
                "tasks": tasks_total
            },
            "by_status": {
                "DONE": by_status.get("DONE").copied().unwrap_or(0),
                "ACTIVE": by_status.get("ACTIVE").copied().unwrap_or(0),
                "TODO": by_status.get("TODO").copied().unwrap_or(0)
            },
            "plans": plans_out,
            "tasks": tasks_out,
            "plans_pagination": {
                "cursor": plans_cursor,
                "next_cursor": if plans_cursor + plans_limit < plans_total as usize { Some(plans_cursor + plans_limit) } else { None },
                "total": plans_total,
                "count": plans_out.len(),
                "limit": plans_limit
            },
            "tasks_pagination": {
                "cursor": tasks_cursor,
                "next_cursor": if tasks_cursor + tasks_limit < tasks_total as usize { Some(tasks_cursor + tasks_limit) } else { None },
                "total": tasks_total,
                "count": tasks_out.len(),
                "limit": tasks_limit
            }
        });

        redact_value(&mut result, 6);

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |= compact_tasks_context_items(&mut result);
            }
            let (_used, tasks_truncated) = enforce_graph_list_budget(&mut result, "tasks", limit);
            let (_used, plans_truncated) = enforce_graph_list_budget(&mut result, "plans", limit);
            truncated |= tasks_truncated || plans_truncated;
            if json_len_chars(&result) > limit {
                truncated |= compact_tasks_context_pagination(&mut result);
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["tasks"], false);
                        changed |= retain_one_at(value, &["plans"], false);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["tasks", "plans"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |=
                            drop_fields_at(value, &[], &["plans_pagination", "tasks_pagination"]);
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("context", result)
        } else {
            ai_ok_with_warnings("context", result, warnings, Vec::new())
        }
    }
}
