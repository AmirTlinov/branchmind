#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_export(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let target_id = match require_string(args_obj, "target") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let notes_limit = match optional_usize(args_obj, "notes_limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let trace_limit = match optional_usize(args_obj, "trace_limit") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let kind = match parse_plan_or_task_kind(&target_id) {
            Some(v) => v,
            None => return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-"),
        };

        let target = match kind {
            TaskKind::Plan => match self.store.get_plan(&workspace, &target_id) {
                Ok(Some(p)) => json!({
                    "id": p.id,
                    "kind": "plan",
                    "revision": p.revision,
                    "title": p.title,
                    "created_at_ms": p.created_at_ms,
                    "updated_at_ms": p.updated_at_ms
                }),
                Ok(None) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown target id",
                        Some("Reveal full toolset to list available ids, then retry."),
                        vec![suggest_call(
                            "tasks_context",
                            "List plans and tasks for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
            TaskKind::Task => match self.store.get_task(&workspace, &target_id) {
                Ok(Some(t)) => json!({
                    "id": t.id,
                    "kind": "task",
                    "revision": t.revision,
                    "parent": t.parent_plan_id,
                    "title": t.title,
                    "created_at_ms": t.created_at_ms,
                    "updated_at_ms": t.updated_at_ms
                }),
                Ok(None) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown target id",
                        Some("Reveal full toolset to list available ids, then retry."),
                        vec![suggest_call(
                            "tasks_context",
                            "List plans and tasks for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        };

        let reasoning = match self
            .store
            .ensure_reasoning_ref(&workspace, &target_id, kind)
        {
            Ok(r) => r,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown target id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let branch = reasoning.branch.clone();
        let notes_doc = reasoning.notes_doc.clone();
        let trace_doc = reasoning.trace_doc.clone();

        let notes_slice =
            match self
                .store
                .doc_show_tail(&workspace, &branch, &notes_doc, None, notes_limit)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
        let trace_slice =
            match self
                .store
                .doc_show_tail(&workspace, &branch, &trace_doc, None, trace_limit)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

        let notes_entries = doc_entries_to_json(notes_slice.entries);
        let trace_entries = doc_entries_to_json(trace_slice.entries);

        let notes_count = notes_entries.len();
        let trace_count = trace_entries.len();
        let notes_branch = branch.clone();
        let trace_branch = branch.clone();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "target": target,
            "reasoning_ref": {
                "branch": reasoning.branch,
                "notes_doc": reasoning.notes_doc,
                "graph_doc": reasoning.graph_doc,
                "trace_doc": reasoning.trace_doc
            },
            "notes": {
                "branch": notes_branch,
                "doc": notes_doc,
                "entries": notes_entries,
                "pagination": {
                    "cursor": Value::Null,
                    "next_cursor": notes_slice.next_cursor,
                    "has_more": notes_slice.has_more,
                    "limit": notes_limit,
                    "count": notes_count
                }
            },
            "trace": {
                "branch": trace_branch,
                "doc": trace_doc,
                "entries": trace_entries,
                "pagination": {
                    "cursor": Value::Null,
                    "next_cursor": trace_slice.next_cursor,
                    "has_more": trace_slice.has_more,
                    "limit": trace_limit,
                    "count": trace_count
                }
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
                truncated |= compact_doc_entries_at(
                    &mut result,
                    &["notes", "entries"],
                    256,
                    true,
                    false,
                    true,
                );
                truncated |= compact_doc_entries_at(
                    &mut result,
                    &["trace", "entries"],
                    256,
                    true,
                    false,
                    true,
                );
            }
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["notes", "entries"], limit, true);
                refresh_pagination_count(
                    &mut result,
                    &["notes", "entries"],
                    &["notes", "pagination"],
                );
            }
            if json_len_chars(&result) > limit {
                truncated |= trim_array_to_budget(&mut result, &["trace", "entries"], limit, true);
                refresh_pagination_count(
                    &mut result,
                    &["trace", "entries"],
                    &["trace", "pagination"],
                );
            }
            if json_len_chars(&result) > limit && compact_target_for_budget(&mut result) {
                truncated = true;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= compact_doc_entries_at(
                            value,
                            &["notes", "entries"],
                            128,
                            true,
                            true,
                            true,
                        );
                        changed |= compact_doc_entries_at(
                            value,
                            &["trace", "entries"],
                            128,
                            true,
                            true,
                            true,
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |= minimalize_doc_entries_at(value, &["notes", "entries"]);
                        changed |= minimalize_doc_entries_at(value, &["trace", "entries"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["notes", "entries"], true);
                        changed |= retain_one_at(value, &["trace", "entries"], true);
                        refresh_pagination_count(
                            value,
                            &["notes", "entries"],
                            &["notes", "pagination"],
                        );
                        refresh_pagination_count(
                            value,
                            &["trace", "entries"],
                            &["trace", "pagination"],
                        );
                    }
                    if json_len_chars(value) > limit {
                        changed |=
                            drop_fields_at(value, &["notes"], &["pagination", "branch", "doc"]);
                        changed |=
                            drop_fields_at(value, &["trace"], &["pagination", "branch", "doc"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &["reasoning_ref"], &["graph_doc"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["trace"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["notes"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("export", result)
        } else {
            ai_ok_with_warnings("export", result, warnings, Vec::new())
        }
    }
}
