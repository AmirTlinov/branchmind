#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_branch_rename(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let from = match require_string(args_obj, "old") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to = match require_string(args_obj, "new") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (previous, current) = match self.store.branch_rename(&workspace, &from, &to) {
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
            Err(StoreError::BranchAlreadyExists) => {
                return ai_error("INVALID_INPUT", "branch already exists");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branch_rename",
            json!({
                "workspace": workspace.as_str(),
                "previous": previous,
                "current": current
            }),
        )
    }
}
