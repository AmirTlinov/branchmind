#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_docs_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let reference = match optional_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let ref_name = match reference {
            Some(v) => v,
            None => match require_checkout_branch(&mut self.store, &workspace) {
                Ok(v) => v,
                Err(resp) => return resp,
            },
        };

        let tag = match self.store.vcs_tag_get(&workspace, &ref_name) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let branch = if self
            .store
            .branch_exists(&workspace, &ref_name)
            .unwrap_or(false)
        {
            ref_name.clone()
        } else if let Some(tag) = tag.as_ref() {
            tag.branch.clone()
        } else {
            return ai_error("UNKNOWN_ID", "Unknown ref");
        };

        let docs = match self.store.doc_list(&workspace, &branch) {
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

        let docs_json = docs
            .into_iter()
            .map(|doc| {
                json!({
                    "doc": doc.doc,
                    "kind": doc.kind.as_str(),
                    "created_at_ms": doc.created_at_ms,
                    "updated_at_ms": doc.updated_at_ms
                })
            })
            .collect::<Vec<_>>();
        let docs_count = docs_json.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "ref": ref_name,
            "branch": branch,
            "docs": docs_json,
            "count": docs_count,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "docs", limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "docs", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("docs_list", result)
    }
}
