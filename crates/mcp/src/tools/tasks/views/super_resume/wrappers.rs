#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_snapshot(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let mut patched = args_obj.clone();
        patched
            .entry("graph_diff".to_string())
            .or_insert_with(|| Value::Bool(true));

        let mut response = self.tool_tasks_resume_super(Value::Object(patched));
        if let Some(obj) = response.as_object_mut() {
            obj.insert("intent".to_string(), Value::String("snapshot".to_string()));
        }
        response
    }

    pub(crate) fn tool_tasks_context_pack(&mut self, args: Value) -> Value {
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
        let delta_limit = args_obj
            .get("delta_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let context = match build_radar_context_with_options(
            &mut self.store,
            &workspace,
            &target_id,
            kind,
            read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut events = if delta_limit == 0 {
            Vec::new()
        } else {
            match self
                .store
                .list_events_for_task(&workspace, &target_id, delta_limit)
            {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };
        events.reverse();
        sort_events_by_seq(&mut events);
        let events_total = events.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "target": context.target,
            "radar": context.radar,
            "delta": {
                "limit": delta_limit,
                "events": events_to_json(events)
            }
        });
        if let Some(steps) = context.steps
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("steps".to_string(), steps);
        }

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |= compact_event_payloads_at(&mut result, &["delta", "events"]);
            }
            truncated |= trim_array_to_budget(&mut result, &["delta", "events"], limit, true);
            let events_empty = result
                .get("delta")
                .and_then(|v| v.get("events"))
                .and_then(|v| v.as_array())
                .map(|events| events.is_empty())
                .unwrap_or(true);
            if events_empty
                && events_total > 0
                && ensure_minimal_list_at(&mut result, &["delta", "events"], events_total, "events")
            {
                truncated = true;
                minimal = true;
            }
            if json_len_chars(&result) > limit {
                let mut removed_any = false;
                if let Some(first) = result
                    .get_mut("steps")
                    .and_then(|v| v.as_object_mut())
                    .and_then(|steps| steps.get_mut("first_open"))
                    .and_then(|v| v.as_object_mut())
                {
                    for key in [
                        "criteria_confirmed",
                        "tests_confirmed",
                        "security_confirmed",
                        "perf_confirmed",
                        "docs_confirmed",
                    ] {
                        removed_any |= first.remove(key).is_some();
                    }
                }
                truncated |= removed_any;
            }
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["steps"], limit, false);
            }
            let (_used, trimmed_fields) = enforce_max_chars_budget(&mut result, limit);
            truncated |= trimmed_fields;
            if json_len_chars(&result) > limit {
                if compact_radar_for_budget(&mut result) {
                    truncated = true;
                }
                if compact_target_for_budget(&mut result) {
                    truncated = true;
                }
            }
            if json_len_chars(&result) > limit {
                let removed = result
                    .get_mut("radar")
                    .and_then(|v| v.as_object_mut())
                    .map(|radar| radar.remove("why").is_some())
                    .unwrap_or(false);
                truncated |= removed;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    changed |= compact_event_payloads_at(value, &["delta", "events"]);
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["delta", "events"], true);
                    }
                    if json_len_chars(value) > limit {
                        changed |= ensure_minimal_list_at(
                            value,
                            &["delta", "events"],
                            events_total,
                            "events",
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["steps"], &["first_open"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["steps"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= compact_radar_for_budget(value);
                        changed |= compact_target_for_budget(value);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(
                            value,
                            &["radar"],
                            &["why", "verify", "next", "blockers"],
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["delta"], &["events"]);
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("context_pack", result)
        } else {
            ai_ok_with_warnings("context_pack", result, warnings, Vec::new())
        }
    }
}
