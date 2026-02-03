#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_validate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let target = args_obj
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

        let (branch, doc) =
            match super::resolve_target_or_branch_doc(self, &workspace, target, branch, doc) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let max_errors = match optional_usize(args_obj, "max_errors") {
            Ok(v) => v.unwrap_or(50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let validation = match self
            .store
            .graph_validate(&workspace, &branch, &doc, max_errors)
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

        let errors = validation
            .errors
            .into_iter()
            .map(|e| {
                json!({
                    "code": e.code,
                    "message": e.message,
                    "kind": e.kind,
                    "key": e.key,
                    "recovery": "Inspect the referenced node/edge via think_query or graph_query, fix with graph_apply, then re-run think_lint."
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": doc,
            "ok": validation.ok,
            "stats": { "nodes": validation.nodes, "edges": validation.edges },
            "errors": errors,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "errors", limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "errors", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("graph_validate", result)
    }
}
