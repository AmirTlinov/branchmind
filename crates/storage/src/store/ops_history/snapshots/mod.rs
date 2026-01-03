#![forbid(unsafe_code)]

mod parse;
mod step_block;
mod step_patch;
mod step_progress;
mod task_detail;
mod task_node;

use super::super::OpsHistoryTarget;
use super::super::StoreError;
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;
use serde_json::Value as JsonValue;

pub(super) fn apply_ops_history_snapshot_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    intent: &str,
    snapshot: &JsonValue,
    now_ms: i64,
) -> Result<OpsHistoryTarget, StoreError> {
    match intent {
        "task_detail_patch" => {
            task_detail::apply_task_detail_snapshot_tx(tx, workspace, snapshot, now_ms)
        }
        "step_patch" => step_patch::apply_step_patch_snapshot_tx(tx, workspace, snapshot, now_ms),
        "step_progress" => {
            step_progress::apply_step_progress_snapshot_tx(tx, workspace, snapshot, now_ms)
        }
        "step_block_set" => {
            step_block::apply_step_block_snapshot_tx(tx, workspace, snapshot, now_ms)
        }
        "task_node_patch" => {
            task_node::apply_task_node_snapshot_tx(tx, workspace, snapshot, now_ms)
        }
        _ => Err(StoreError::InvalidInput("undo not supported for intent")),
    }
}
