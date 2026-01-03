#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_delta(&mut self, args: Value) -> Value {
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
        let since = args_obj.get("since").and_then(|v| v.as_str());
        let limit = args_obj
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);

        let events = match self.store.list_events(&workspace, since, limit) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "events": events.into_iter().map(|e| json!({
                "event_id": e.event_id(),
                "ts": ts_ms_to_rfc3339(e.ts_ms),
                "ts_ms": e.ts_ms,
                "task": e.task_id,
                "path": e.path,
                "type": e.event_type,
                "payload": parse_json_or_string(&e.payload_json),
            })).collect::<Vec<_>>()
        });

        redact_value(&mut result, 6);

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |= compact_event_payloads_at(&mut result, &["events"]);
            }
            let (_used, events_truncated) = enforce_graph_list_budget(&mut result, "events", limit);
            truncated |= events_truncated;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= minimalize_task_events_at(value, &["events"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["events"], true);
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("delta", result)
        } else {
            ai_ok_with_warnings("delta", result, warnings, Vec::new())
        }
    }
}
