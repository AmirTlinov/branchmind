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
            Ok(v) => v.unwrap_or(200).clamp(1, 500),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // Detect truncation precisely by probing one extra item (bounded by store hard cap).
        let probe_limit = limit.saturating_add(1);
        let mut branches = match self.store.branch_list(&workspace, probe_limit) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let mut truncated_by_limit = false;
        if branches.len() > limit {
            truncated_by_limit = true;
            branches.truncate(limit);
        }

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
            "truncated": truncated_by_limit
        });

        if let Some(limit) = max_chars {
            let (_used, budget_truncated) =
                enforce_branchmind_branch_list_budget(&mut result, limit);
            if let Some(obj) = result.as_object_mut()
                && let Some(branches) = obj.get("branches").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(branches.len() as u64)),
                );
            }

            let truncated = truncated_by_limit || budget_truncated;
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) =
                    enforce_branchmind_branch_list_budget(&mut result, limit);
                if let Some(obj) = result.as_object_mut()
                    && let Some(branches) = obj.get("branches").and_then(|v| v.as_array())
                {
                    obj.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(branches.len() as u64)),
                    );
                }
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("branch_list", result)
    }
}
