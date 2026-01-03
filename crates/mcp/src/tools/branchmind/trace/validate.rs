#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_trace_validate(&mut self, args: Value) -> Value {
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

        let (branch, trace_doc) = match self.resolve_trace_scope_with_ref(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !self
            .store
            .branch_exists(&workspace, &branch)
            .unwrap_or(false)
        {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let trace_slice = match self
            .store
            .doc_show_tail(&workspace, &branch, &trace_doc, None, 200)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut errors = Vec::new();
        for entry in &trace_slice.entries {
            if entry.format.as_deref() != Some("trace_sequential_step") {
                continue;
            }
            let meta_value = entry
                .meta_json
                .as_ref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
            let Some(Value::Object(meta_obj)) = meta_value else {
                errors.push(json!({
                    "seq": entry.seq,
                    "code": "missing_meta",
                    "message": "trace_sequential_step requires object meta"
                }));
                continue;
            };
            let thought_number = meta_obj.get("thoughtNumber").and_then(|v| v.as_i64());
            let total_thoughts = meta_obj.get("totalThoughts").and_then(|v| v.as_i64());
            let next_thought_needed = meta_obj.get("nextThoughtNeeded").and_then(|v| v.as_bool());

            if thought_number.unwrap_or(0) <= 0 {
                errors.push(json!({
                    "seq": entry.seq,
                    "code": "invalid_thought_number",
                    "message": "thoughtNumber must be positive"
                }));
            }
            if total_thoughts.unwrap_or(0) <= 0
                || (thought_number.is_some()
                    && total_thoughts.is_some()
                    && total_thoughts.unwrap_or(0) < thought_number.unwrap_or(0))
            {
                errors.push(json!({
                    "seq": entry.seq,
                    "code": "invalid_total_thoughts",
                    "message": "totalThoughts must be positive and >= thoughtNumber"
                }));
            }
            if next_thought_needed.is_none() {
                errors.push(json!({
                    "seq": entry.seq,
                    "code": "missing_next_thought_needed",
                    "message": "nextThoughtNeeded is required"
                }));
            }
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": trace_doc,
            "ok": errors.is_empty(),
            "checked": trace_slice.entries.len(),
            "has_more": trace_slice.has_more,
            "errors": errors,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let (_used, trimmed_fields) = enforce_max_chars_budget(&mut result, limit);
            truncated |= trimmed_fields;
            if json_len_chars(&result) > limit {
                let (_used, errors_trimmed) =
                    enforce_graph_list_budget(&mut result, "errors", limit);
                truncated |= errors_trimmed;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["errors"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("trace_validate", result)
        } else {
            ai_ok_with_warnings("trace_validate", result, warnings, Vec::new())
        }
    }
}
