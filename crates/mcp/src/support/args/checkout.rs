#![forbid(unsafe_code)]

use super::super::ai::{ai_error, ai_error_with, format_store_error, suggest_call};
use bm_core::ids::WorkspaceId;
use bm_storage::SqliteStore;
use serde_json::{Value, json};

pub(crate) fn require_checkout_branch(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
) -> Result<String, Value> {
    match store.branch_checkout_get(workspace) {
        Ok(Some(branch)) => Ok(branch),
        Ok(None) => Err(ai_error_with(
            "INVALID_INPUT",
            "Checkout branch is not set",
            Some(
                "Use status (portal) to auto-init and see checkout. Reveal full toolset to list branches if needed.",
            ),
            vec![
                suggest_call(
                    "init",
                    "Initialize the workspace and bootstrap a default branch.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                ),
                suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "medium",
                    json!({ "workspace": workspace.as_str() }),
                ),
            ],
        )),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}

pub(crate) fn unknown_branch_error(workspace: &WorkspaceId) -> Value {
    ai_error_with(
        "UNKNOWN_ID",
        "Unknown branch",
        Some("Reveal full toolset to list branches, then retry."),
        vec![suggest_call(
            "branch_list",
            "List known branches for this workspace.",
            "high",
            json!({ "workspace": workspace.as_str() }),
        )],
    )
}
