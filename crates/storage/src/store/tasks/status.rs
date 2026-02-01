#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn set_plan_status(
        &mut self,
        workspace: &WorkspaceId,
        request: SetPlanStatusRequest,
    ) -> Result<(i64, EventRow), StoreError> {
        let SetPlanStatusRequest {
            id,
            expected_revision,
            status,
            status_manual,
            event_type,
            event_payload_json,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                "SELECT revision, status FROM plans WHERE workspace = ?1 AND id = ?2",
                params![workspace.as_str(), &id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((revision, _current_status)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision
            && expected != revision
        {
            return Err(StoreError::RevisionMismatch {
                expected,
                actual: revision,
            });
        }

        let new_revision = revision + 1;
        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3, status = ?4, status_manual = ?5, updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                &id,
                new_revision,
                &status,
                if status_manual { 1i64 } else { 0i64 },
                now_ms
            ],
        )?;

        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &id,
                kind: TaskKind::Plan,
                path: None,
                event_type: &event_type,
                payload_json: &event_payload_json,
            },
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }

    pub fn set_task_status(
        &mut self,
        workspace: &WorkspaceId,
        request: SetTaskStatusRequest,
    ) -> Result<(i64, EventRow), StoreError> {
        let SetTaskStatusRequest {
            id,
            expected_revision,
            status,
            parked_until_ts_ms,
            status_manual,
            require_steps_completed,
            event_type,
            event_payload_json,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                "SELECT revision, status FROM tasks WHERE workspace = ?1 AND id = ?2",
                params![workspace.as_str(), &id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((revision, _current_status)) = row else {
            return Err(StoreError::UnknownId);
        };

        if let Some(expected) = expected_revision
            && expected != revision
        {
            return Err(StoreError::RevisionMismatch {
                expected,
                actual: revision,
            });
        }

        if require_steps_completed {
            let open_steps: i64 = tx.query_row(
                "SELECT COUNT(*) FROM steps WHERE workspace=?1 AND task_id=?2 AND completed=0",
                params![workspace.as_str(), &id],
                |row| row.get(0),
            )?;
            if open_steps > 0 {
                return Err(StoreError::InvalidInput("steps not completed"));
            }
        }

        let new_revision = revision + 1;
        let parked_until_ts_ms = if status == "PARKED" {
            parked_until_ts_ms
        } else {
            None
        };
        tx.execute(
            r#"
            UPDATE tasks
            SET revision = ?3, status = ?4, parked_until_ts_ms = ?5, status_manual = ?6, updated_at_ms = ?7
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                &id,
                new_revision,
                &status,
                parked_until_ts_ms,
                if status_manual { 1i64 } else { 0i64 },
                now_ms
            ],
        )?;

        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &id,
                kind: TaskKind::Task,
                path: None,
                event_type: &event_type,
                payload_json: &event_payload_json,
            },
        )?;

        tx.commit()?;
        Ok((new_revision, event))
    }
}
