#![forbid(unsafe_code)]

use super::super::{EventRow, OpsHistoryRow, SqliteStore, StoreError};
use super::snapshots;
use bm_core::ids::WorkspaceId;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use rusqlite::params;
use serde_json::Value as JsonValue;

impl SqliteStore {
    pub fn ops_history_undo(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        self.ops_history_apply(workspace, task_id, true)
    }

    pub fn ops_history_redo(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        self.ops_history_apply(workspace, task_id, false)
    }

    fn ops_history_apply(
        &mut self,
        workspace: &WorkspaceId,
        task_id: Option<&str>,
        undo: bool,
    ) -> Result<(OpsHistoryRow, EventRow), StoreError> {
        let now_ms = super::super::now_ms();
        let tx = self.conn.transaction()?;
        super::super::ensure_workspace_tx(&tx, workspace, now_ms)?;

        let row = select_undoable_op_tx(&tx, workspace, task_id, undo)?
            .ok_or(StoreError::InvalidInput("no undoable operations"))?;

        let snapshot = parse_snapshot_from_row(&row, undo)?;
        let target = snapshots::apply_ops_history_snapshot_tx(
            &tx,
            workspace,
            row.intent.as_str(),
            &snapshot,
            now_ms,
        )?;

        update_undone_flag_tx(&tx, workspace, row.seq, undo)?;

        let event_payload_json = super::super::build_undo_redo_payload(
            row.seq,
            row.intent.as_str(),
            row.task_id.as_deref(),
            row.path.as_deref(),
            undo,
        );
        let event = super::super::insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            row.task_id.clone(),
            row.path.clone(),
            if undo { "undo_applied" } else { "redo_applied" },
            &event_payload_json,
        )?;

        if let Some(task_id) = row.task_id.as_deref() {
            let kind = super::super::parse_plan_or_task_kind(task_id)?;
            let reasoning_ref =
                super::super::ensure_reasoning_ref_tx(&tx, workspace, task_id, kind, now_ms)?;
            let _ = super::super::ingest_task_event_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.trace_doc,
                &event,
            )?;

            match target {
                super::super::OpsHistoryTarget::Task { title } => {
                    if let Some(title) = title {
                        let touched = Self::project_task_graph_task_node_tx(
                            &tx,
                            workspace.as_str(),
                            &reasoning_ref,
                            &event,
                            task_id,
                            &title,
                            now_ms,
                        )?;
                        if touched {
                            super::super::touch_document_tx(
                                &tx,
                                workspace.as_str(),
                                &reasoning_ref.branch,
                                &reasoning_ref.graph_doc,
                                now_ms,
                            )?;
                        }
                    }
                }
                super::super::OpsHistoryTarget::Step { step } => {
                    let (snapshot_title, snapshot_completed) = super::super::step_snapshot_tx(
                        &tx,
                        workspace.as_str(),
                        task_id,
                        &step.step_id,
                    )?;
                    let graph_touched = Self::project_task_graph_step_node_tx(
                        &tx,
                        workspace.as_str(),
                        &reasoning_ref,
                        &event,
                        task_id,
                        &step,
                        &snapshot_title,
                        snapshot_completed,
                        now_ms,
                    )?;
                    if graph_touched {
                        super::super::touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &reasoning_ref.branch,
                            &reasoning_ref.graph_doc,
                            now_ms,
                        )?;
                    }
                }
                super::super::OpsHistoryTarget::TaskNode => {}
            }
        }

        tx.commit()?;
        Ok((row, event))
    }
}

fn select_undoable_op_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    task_id: Option<&str>,
    undo: bool,
) -> Result<Option<OpsHistoryRow>, StoreError> {
    let undone_flag = if undo { 0i64 } else { 1i64 };
    let mut stmt = tx.prepare(
        r#"
        SELECT seq, ts_ms, task_id, path, intent, payload_json, before_json, after_json, undoable, undone
        FROM ops_history
        WHERE workspace=?1 AND undoable=1 AND undone=?2 AND (?3 IS NULL OR task_id=?3)
        ORDER BY seq DESC
        LIMIT 1
        "#,
    )?;
    Ok(stmt
        .query_row(params![workspace.as_str(), undone_flag, task_id], |row| {
            Ok(OpsHistoryRow {
                seq: row.get(0)?,
                ts_ms: row.get(1)?,
                task_id: row.get(2)?,
                path: row.get(3)?,
                intent: row.get(4)?,
                payload_json: row.get(5)?,
                before_json: row.get(6)?,
                after_json: row.get(7)?,
                undoable: row.get::<_, i64>(8)? != 0,
                undone: row.get::<_, i64>(9)? != 0,
            })
        })
        .optional()?)
}

fn parse_snapshot_from_row(row: &OpsHistoryRow, undo: bool) -> Result<JsonValue, StoreError> {
    let snapshot_json = if undo {
        row.before_json.as_deref()
    } else {
        row.after_json.as_deref()
    }
    .ok_or(StoreError::InvalidInput("snapshot missing"))?;
    serde_json::from_str(snapshot_json).map_err(|_| StoreError::InvalidInput("snapshot invalid"))
}

fn update_undone_flag_tx(
    tx: &Transaction<'_>,
    workspace: &WorkspaceId,
    seq: i64,
    undo: bool,
) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE ops_history SET undone=?3 WHERE workspace=?1 AND seq=?2",
        params![workspace.as_str(), seq, if undo { 1i64 } else { 0i64 }],
    )?;
    Ok(())
}
