#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_trace_hydrate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit_steps = match optional_usize(args_obj, "limit_steps") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let statement_max_bytes = match optional_usize(args_obj, "statement_max_bytes") {
            Ok(v) => v,
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
        let trace_slice =
            match self
                .store
                .doc_show_tail(&workspace, &branch, &trace_doc, None, limit_steps)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        let mut entries = doc_entries_to_json(trace_slice.entries);
        let entries_count = entries.len();

        if let Some(max_bytes) = statement_max_bytes {
            for entry in &mut entries {
                if let Some(content) = entry.get("content").and_then(|v| v.as_str()) {
                    let trimmed = truncate_string_bytes(content, max_bytes);
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("content".to_string(), Value::String(trimmed));
                    }
                }
            }
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": trace_doc,
            "entries": entries,
            "pagination": {
                "cursor": Value::Null,
                "next_cursor": trace_slice.next_cursor,
                "has_more": trace_slice.has_more,
                "limit": limit_steps,
                "count": entries_count
            },
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |=
                    compact_doc_entries_at(&mut result, &["entries"], 256, true, false, true);
            }
            let (_used, trimmed_fields) = enforce_max_chars_budget(&mut result, limit);
            truncated |= trimmed_fields;
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["entries"], limit, true);
                refresh_pagination_count(&mut result, &["entries"], &["pagination"]);
            }
            if json_len_chars(&result) > limit {
                let minimized = minimalize_doc_entries_at(&mut result, &["entries"]);
                if minimized {
                    truncated = true;
                    minimal = true;
                }
            }
            if json_len_chars(&result) > limit {
                let retained = retain_one_at(&mut result, &["entries"], true);
                if retained {
                    truncated = true;
                    minimal = true;
                    refresh_pagination_count(&mut result, &["entries"], &["pagination"]);
                }
            }
            if json_len_chars(&result) > limit {
                truncated |= drop_fields_at(
                    &mut result,
                    &["pagination"],
                    &["cursor", "next_cursor", "has_more", "limit"],
                );
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["pagination"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("trace_hydrate", result)
        } else {
            ai_ok_with_warnings("trace_hydrate", result, warnings, Vec::new())
        }
    }
}
