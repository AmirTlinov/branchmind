#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_show(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let mut target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if target.is_none() && branch.is_none() && doc.is_none() {
            let focus = match self.store.focus_get(&workspace) {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            target = focus;
        }

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let doc_kind = args_obj
            .get("doc_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("trace");
        if doc_kind != "trace" && doc_kind != "notes" {
            return ai_error("INVALID_INPUT", "doc_kind must be 'trace' or 'notes'");
        }

        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let doc = match doc_kind {
                    "trace" => reasoning.trace_doc,
                    "notes" => reasoning.notes_doc,
                    _ => return ai_error("INVALID_INPUT", "doc_kind must be 'trace' or 'notes'"),
                };
                (reasoning.branch, doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| match doc_kind {
                    "notes" => DEFAULT_NOTES_DOC.to_string(),
                    _ => DEFAULT_TRACE_DOC.to_string(),
                });
                (branch, doc)
            }
        };

        let slice = match self
            .store
            .doc_show_tail(&workspace, &branch, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entries = doc_entries_to_json(slice.entries);
        let entries_count = entries.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "entries": entries,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": entries_count
            },
            "truncated": false
        });

        redact_value(&mut result, 6);

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            if json_len_chars(&result) > limit {
                truncated |=
                    compact_doc_entries_at(&mut result, &["entries"], 256, true, false, true);
            }
            let (_used, truncated_show) = enforce_branchmind_show_budget(&mut result, limit);
            truncated |= truncated_show;
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["entries"], limit, true);
                refresh_pagination_count(&mut result, &["entries"], &["pagination"]);
            }
            let entries_empty = result
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if entries_empty
                && entries_count > 0
                && ensure_minimal_list_at(&mut result, &["entries"], entries_count, "entries")
            {
                truncated = true;
                minimal = true;
                set_pagination_total_at(&mut result, &["pagination"], entries_count);
            }
            if json_len_chars(&result) > limit
                && minimalize_doc_entries_at(&mut result, &["entries"])
            {
                truncated = true;
                minimal = true;
            }
            if json_len_chars(&result) > limit && retain_one_at(&mut result, &["entries"], true) {
                truncated = true;
                minimal = true;
                refresh_pagination_count(&mut result, &["entries"], &["pagination"]);
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
                    if json_len_chars(value) > limit
                        && ensure_minimal_list_at(value, &["entries"], entries_count, "entries")
                    {
                        changed = true;
                        set_pagination_total_at(value, &["pagination"], entries_count);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("show", result)
        } else {
            ai_ok_with_warnings("show", result, warnings, Vec::new())
        }
    }
}
