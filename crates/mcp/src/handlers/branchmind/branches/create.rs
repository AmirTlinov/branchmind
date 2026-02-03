#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_branch_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let name = match require_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from = match optional_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let info = match self.store.branch_create(&workspace, &name, from.as_deref()) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown base branch",
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
                return ai_error_with(
                    "CONFLICT",
                    "Branch already exists",
                    Some("Choose a different name (or delete/rename the existing branch)."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
            Err(StoreError::BranchCycle) => return ai_error("INVALID_INPUT", "Branch base cycle"),
            Err(StoreError::BranchDepthExceeded) => {
                return ai_error("INVALID_INPUT", "Branch base depth exceeded");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "branch_create",
            json!({
                "workspace": workspace.as_str(),
                "branch": {
                    "name": info.name,
                    "base_branch": info.base_branch,
                    "base_seq": info.base_seq
                }
            }),
        )
    }
}
