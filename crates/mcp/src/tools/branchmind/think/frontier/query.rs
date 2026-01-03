#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn graph_query_cards(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    graph_doc: &str,
    request: bm_storage::GraphQueryRequest,
) -> Result<Vec<Value>, Value> {
    match server
        .store
        .graph_query(workspace, branch, graph_doc, request)
    {
        Ok(v) => Ok(graph_nodes_to_cards(v.nodes)),
        Err(StoreError::UnknownBranch) => Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown branch",
            Some("Call branch_list to discover existing branches, then retry."),
            vec![suggest_call(
                "branch_list",
                "List known branches for this workspace.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            )],
        )),
        Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}
