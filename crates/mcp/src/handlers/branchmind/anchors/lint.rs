#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_lint(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let lint = match self
            .store
            .anchors_lint(&workspace, bm_storage::AnchorsLintRequest { limit })
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let issues_json = lint
            .issues
            .into_iter()
            .map(|i| {
                json!({
                    "code": i.code,
                    "severity": i.severity,
                    "anchor": i.anchor,
                    "message": i.message,
                    "hint": i.hint
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "issues": issues_json,
            "count": issues_json.len(),
            "has_more": lint.has_more,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "issues", limit);

            if let Some(obj) = result.as_object_mut()
                && let Some(issues) = obj.get("issues").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(issues.len() as u64)),
                );
            }

            set_truncated_flag(&mut result, budget_truncated);
            let _used = attach_budget(&mut result, limit, budget_truncated);

            let warnings = budget_warnings(budget_truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("anchors_lint", result)
            } else {
                ai_ok_with_warnings("anchors_lint", result, warnings, Vec::new())
            }
        } else {
            ai_ok("anchors_lint", result)
        }
    }
}
