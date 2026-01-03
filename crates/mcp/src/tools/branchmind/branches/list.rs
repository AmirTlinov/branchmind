#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_branch_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let branches = match self.store.branch_list(&workspace, limit) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let branches_json = branches
            .into_iter()
            .map(|b| {
                json!({
                    "name": b.name,
                    "base_branch": b.base_branch,
                    "base_seq": b.base_seq,
                    "created_at_ms": b.created_at_ms
                })
            })
            .collect::<Vec<_>>();

        let count = branches_json.len();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "branches": branches_json,
            "count": count,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_branchmind_branch_list_budget(&mut result, limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) =
                    enforce_branchmind_branch_list_budget(&mut result, limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branch_list", result)
    }
}
