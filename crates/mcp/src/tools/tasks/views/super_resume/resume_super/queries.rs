#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

fn empty_slice() -> bm_storage::GraphQuerySlice {
    bm_storage::GraphQuerySlice {
        nodes: Vec::new(),
        edges: Vec::new(),
        next_cursor: None,
        has_more: false,
    }
}

pub(super) fn graph_query_or_empty(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
    request: bm_storage::GraphQueryRequest,
    read_only: bool,
    reasoning_branch_missing: &mut bool,
) -> Result<bm_storage::GraphQuerySlice, Value> {
    match server.store.graph_query(workspace, branch, doc, request) {
        Ok(v) => Ok(v),
        Err(StoreError::UnknownBranch) => {
            if read_only {
                *reasoning_branch_missing = true;
                Ok(empty_slice())
            } else {
                Err(unknown_branch_error(workspace))
            }
        }
        Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}
