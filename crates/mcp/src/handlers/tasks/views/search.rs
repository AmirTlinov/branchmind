#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

const DEFAULT_LIMIT: usize = 12;
const MAX_LIMIT: usize = 120;
const MAX_QUERY_CHARS: usize = 200;

fn kind_bias(kind: &str) -> i64 {
    match kind {
        "plan" => -2,
        "slice" => -1,
        _ => 0,
    }
}

impl McpServer {
    /// Jump/search for tasks and plans (portal UX).
    ///
    /// Intended to avoid "cmd.list → scroll → copy id" loops: return a small list of openable ids
    /// plus actions to open each hit.
    pub(crate) fn tool_tasks_search(&mut self, args: Value) -> Value {
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
        let mut text = match require_string(args_obj, "text") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        text = text.trim().to_string();
        if text.is_empty() {
            return ai_error("INVALID_INPUT", "text must not be empty");
        }

        let mut warnings = Vec::<Value>::new();
        if text.chars().count() > MAX_QUERY_CHARS {
            text = text.chars().take(MAX_QUERY_CHARS).collect::<String>();
            warnings.push(warning(
                "QUERY_TRUNCATED",
                "text query truncated",
                "Use a shorter query (<= 200 chars) for deterministic, bounded results.",
            ));
        }

        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            Err(resp) => return resp,
        };

        let plans = match self.store.search_plans(
            &workspace,
            bm_storage::PlansSearchRequest {
                text: text.clone(),
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tasks = match self.store.search_tasks(
            &workspace,
            bm_storage::TasksSearchRequest {
                text: text.clone(),
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let slices = if self.slice_plans_v1_enabled {
            match self.store.search_plan_slices(
                &workspace,
                bm_storage::PlanSlicesSearchRequest {
                    text: text.clone(),
                    limit,
                },
            ) {
                Ok(v) => Some(v),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            None
        };

        let mut hits = Vec::<Value>::new();
        for plan in plans.plans.into_iter() {
            hits.push(json!({
                "kind": "plan",
                "id": plan.id,
                "title": plan.title,
                "updated_at_ms": plan.updated_at_ms,
            }));
        }
        for task in tasks.tasks.into_iter() {
            hits.push(json!({
                "kind": "task",
                "id": task.id,
                "title": task.title,
                "plan_id": task.plan_id,
                "updated_at_ms": task.updated_at_ms,
            }));
        }
        if let Some(slices) = slices.as_ref() {
            for slice in slices.slices.iter() {
                hits.push(json!({
                    "kind": "slice",
                    "id": slice.slice_id,
                    "title": slice.title,
                    "plan_id": slice.plan_id,
                    "slice_task_id": slice.slice_task_id,
                    "status": slice.status,
                    "objective": slice.objective,
                    "updated_at_ms": slice.updated_at_ms,
                }));
            }
        }

        hits.sort_by(|a, b| {
            let ak = a.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let bk = b.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let at = a.get("updated_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let bt = b.get("updated_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let diff = bt.cmp(&at);
            if diff != std::cmp::Ordering::Equal {
                return diff;
            }
            let bias = kind_bias(ak).cmp(&kind_bias(bk));
            if bias != std::cmp::Ordering::Equal {
                return bias;
            }
            let ai = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let bi = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            ai.cmp(bi)
        });

        let mut has_more = plans.has_more || tasks.has_more;
        if let Some(slices) = slices.as_ref() {
            has_more = has_more || slices.has_more;
        }
        if hits.len() > limit {
            hits.truncate(limit);
            has_more = true;
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "text": text,
            "limit": limit,
            "hits": hits,
            "has_more": has_more,
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            // Prefer trimming hits first (stable navigation payload).
            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |v| {
                    let Some(hits) = v.get_mut("hits").and_then(|vv| vv.as_array_mut()) else {
                        return false;
                    };
                    if hits.is_empty() {
                        return false;
                    }
                    hits.pop();
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("has_more".to_string(), Value::Bool(true));
                    }
                    true
                });

            warnings.extend(budget_warnings(truncated, minimal, clamped));
        }

        let mut suggestions = Vec::<Value>::new();
        if let Some(hits) = result.get("hits").and_then(|v| v.as_array()) {
            for hit in hits {
                if let Some(id) = hit.get("id").and_then(|v| v.as_str()) {
                    suggestions.push(suggest_call(
                        "open",
                        "Open search hit",
                        "medium",
                        json!({ "id": id, "include_content": true }),
                    ));
                }
            }
        }

        if warnings.is_empty() {
            ai_ok_with("tasks_search", result, suggestions)
        } else {
            ai_ok_with_warnings("tasks_search", result, warnings, suggestions)
        }
    }
}
