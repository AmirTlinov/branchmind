#![forbid(unsafe_code)]

use super::super::super::{EventRow, ReasoningRefRow, StoreError};
use super::super::{ensure_reasoning_ref_tx, ingest_task_event_tx};
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{Transaction, params};

pub(in crate::store) fn insert_event_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    ts_ms: i64,
    task_id: Option<String>,
    path: Option<String>,
    event_type: &str,
    payload_json: &str,
) -> Result<EventRow, StoreError> {
    let task_id_for_return = task_id.clone();
    let path_for_return = path.clone();
    tx.execute(
        r#"
        INSERT INTO events(workspace, ts_ms, task_id, path, type, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![workspace, ts_ms, task_id, path, event_type, payload_json],
    )?;
    let seq = tx.last_insert_rowid();
    Ok(EventRow {
        seq,
        ts_ms,
        task_id: task_id_for_return,
        path: path_for_return,
        event_type: event_type.to_string(),
        payload_json: payload_json.to_string(),
    })
}

pub(in crate::store) struct TaskEventEmitTxArgs<'a> {
    pub(in crate::store) workspace: &'a WorkspaceId,
    pub(in crate::store) now_ms: i64,
    pub(in crate::store) task_id: &'a str,
    pub(in crate::store) kind: TaskKind,
    pub(in crate::store) path: Option<String>,
    pub(in crate::store) event_type: &'a str,
    pub(in crate::store) payload_json: &'a str,
}

pub(in crate::store) fn emit_task_event_tx(
    tx: &Transaction<'_>,
    args: TaskEventEmitTxArgs<'_>,
) -> Result<(EventRow, ReasoningRefRow), StoreError> {
    let TaskEventEmitTxArgs {
        workspace,
        now_ms,
        task_id,
        kind,
        path,
        event_type,
        payload_json,
    } = args;
    let event = insert_event_tx(
        tx,
        workspace.as_str(),
        now_ms,
        Some(task_id.to_string()),
        path,
        event_type,
        payload_json,
    )?;

    let reasoning_ref = ensure_reasoning_ref_tx(tx, workspace, task_id, kind, now_ms)?;
    let _ = ingest_task_event_tx(
        tx,
        workspace.as_str(),
        &reasoning_ref.branch,
        &reasoning_ref.trace_doc,
        &event,
    )?;
    Ok((event, reasoning_ref))
}

pub(in crate::store) struct OpsHistoryInsertTxArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) task_id: Option<&'a str>,
    pub(in crate::store) path: Option<String>,
    pub(in crate::store) intent: &'a str,
    pub(in crate::store) payload_json: &'a str,
    pub(in crate::store) before_json: Option<&'a str>,
    pub(in crate::store) after_json: Option<&'a str>,
    pub(in crate::store) undoable: bool,
    pub(in crate::store) now_ms: i64,
}

pub(in crate::store) fn ops_history_insert_tx(
    tx: &Transaction<'_>,
    insert: OpsHistoryInsertTxArgs<'_>,
) -> Result<i64, StoreError> {
    let OpsHistoryInsertTxArgs {
        workspace,
        task_id,
        path,
        intent,
        payload_json,
        before_json,
        after_json,
        undoable,
        now_ms,
    } = insert;

    tx.execute(
        r#"
        INSERT INTO ops_history(workspace, task_id, path, intent, payload_json, before_json, after_json, undoable, undone, ts_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
        "#,
        params![
            workspace,
            task_id,
            path,
            intent,
            payload_json,
            before_json,
            after_json,
            if undoable { 1i64 } else { 0i64 },
            now_ms
        ],
    )?;
    Ok(tx.last_insert_rowid())
}
