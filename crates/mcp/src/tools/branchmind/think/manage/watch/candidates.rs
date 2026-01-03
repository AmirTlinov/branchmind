#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn fetch(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    graph_doc: &str,
    limit_candidates: usize,
) -> Result<Vec<Value>, Value> {
    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();

    let slice = match server.store.graph_query(
        workspace,
        branch,
        graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types),
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: limit_candidates,
            include_edges: false,
            edges_limit: 0,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownBranch) => {
            return Err(ai_error_with(
                "UNKNOWN_ID",
                "Unknown branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ));
        }
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    Ok(graph_nodes_to_cards(slice.nodes))
}
