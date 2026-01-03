#![forbid(unsafe_code)]
//! Graph operations: apply/query/validate/diff (split-friendly).

mod apply;
mod diff;
mod query;
mod validate;

use crate::*;
use serde_json::Value;

fn resolve_target_or_branch_doc(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    target: Option<String>,
    branch: Option<String>,
    doc: Option<String>,
) -> Result<(String, String), Value> {
    let target = if target.is_none() && branch.is_none() && doc.is_none() {
        match server.store.focus_get(workspace) {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
    } else {
        target
    };

    if target.is_some() && (branch.is_some() || doc.is_some()) {
        return Err(ai_error(
            "INVALID_INPUT",
            "provide either target or (branch, doc), not both",
        ));
    }

    match target {
        Some(target_id) => {
            let kind = match parse_plan_or_task_kind(&target_id) {
                Some(v) => v,
                None => {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        "target must start with PLAN- or TASK-",
                    ));
                }
            };
            let reasoning = match server
                .store
                .ensure_reasoning_ref(workspace, &target_id, kind)
            {
                Ok(r) => r,
                Err(StoreError::UnknownId) => {
                    return Err(ai_error("UNKNOWN_ID", "Unknown target id"));
                }
                Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
            };
            Ok((reasoning.branch, reasoning.graph_doc))
        }
        None => {
            let branch = match branch {
                Some(branch) => branch,
                None => require_checkout_branch(&mut server.store, workspace)?,
            };
            let doc = doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
            Ok((branch, doc))
        }
    }
}
