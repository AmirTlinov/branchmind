#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_handoff(&mut self, args: Value) -> Value {
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

        let handoff = match build_handoff_core(&mut self.store, &workspace, &target_id, kind) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "target": context.target,
            "radar": context.radar,
            "handoff": {
                "done": handoff.done,
                "remaining": handoff.remaining,
                "risks": handoff.risks
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

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["steps"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["handoff"], &["risks", "remaining"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(
                            value,
                            &["radar"],
                            &["verify", "next", "blockers", "why"],
                        );
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("handoff", result)
        } else {
            ai_ok_with_warnings("handoff", result, warnings, Vec::new())
        }
    }
}
