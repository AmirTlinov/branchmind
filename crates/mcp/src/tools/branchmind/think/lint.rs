#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_lint(&mut self, args: Value) -> Value {
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

        let (branch, graph_doc) = match self.resolve_think_graph_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let validation = match self
            .store
            .graph_validate(&workspace, &branch, &graph_doc, 50)
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
                    "key": e.key
                })
            })
            .collect::<Vec<_>>();
        let errors_total = errors.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "graph_doc": graph_doc,
            "ok": validation.ok,
            "stats": { "nodes": validation.nodes, "edges": validation.edges },
            "errors": errors,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let (_used, errors_truncated) = enforce_graph_list_budget(&mut result, "errors", limit);
            truncated |= errors_truncated;
            let errors_empty = result
                .get("errors")
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(true);
            if errors_empty
                && errors_total > 0
                && ensure_minimal_list_at(&mut result, &["errors"], errors_total, "errors")
            {
                truncated = true;
                minimal = true;
            }

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= retain_one_at(value, &["errors"], true);
                    }
                    if json_len_chars(value) > limit {
                        changed |=
                            ensure_minimal_list_at(value, &["errors"], errors_total, "errors");
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["errors"]);
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_lint", result)
        } else {
            ai_ok_with_warnings("think_lint", result, warnings, Vec::new())
        }
    }
}
