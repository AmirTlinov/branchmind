#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_diff(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc_kind = args_obj
            .get("doc_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("notes");
        if doc_kind != "notes" && doc_kind != "trace" && doc_kind != "plan_spec" {
            return ai_error(
                "INVALID_INPUT",
                "doc_kind must be 'notes', 'trace', or 'plan_spec'",
            );
        }

        let doc = match optional_string(args_obj, "doc") {
            Ok(Some(v)) => v,
            Ok(None) if doc_kind == "trace" => DEFAULT_TRACE_DOC.to_string(),
            Ok(None) if doc_kind == "plan_spec" => {
                return ai_error("INVALID_INPUT", "doc is required when doc_kind='plan_spec'");
            }
            Ok(None) => "notes".to_string(),
            Err(resp) => return resp,
        };
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

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let to_exists = match self.store.branch_exists(&workspace, &to) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !to_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown to-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let slice = match self
            .store
            .doc_diff_tail(&workspace, &from, &to, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
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
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entries = doc_entries_to_json(slice.entries);
        let plan_spec_diff = if doc_kind == "plan_spec" {
            let from_latest = match super::plan_spec::load_latest_plan_spec(
                &mut self.store,
                &workspace,
                &from,
                &doc,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let to_latest = match super::plan_spec::load_latest_plan_spec(
                &mut self.store,
                &workspace,
                &to,
                &doc,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            super::plan_spec::plan_spec_diff_block(&from, &to, &doc, from_latest, to_latest)
        } else {
            Value::Null
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "from": from,
            "to": to,
            "doc": doc,
            "doc_kind": doc_kind,
            "entries": entries,
            "plan_spec_diff": plan_spec_diff,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": entries.len()
            },
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_show_budget(&mut result, limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_branchmind_show_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("diff", result)
    }
}
