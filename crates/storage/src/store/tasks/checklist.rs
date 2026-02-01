#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn plan_checklist_get(
        &self,
        workspace: &WorkspaceId,
        plan_id: &str,
    ) -> Result<PlanChecklist, StoreError> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT plan_doc, plan_current
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), plan_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((plan_doc, plan_current)) = row else {
            return Err(StoreError::UnknownId);
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT text
            FROM plan_checklist
            WHERE workspace = ?1 AND plan_id = ?2
            ORDER BY ordinal ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str(), plan_id], |row| {
            row.get::<_, String>(0)
        })?;
        let steps = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(PlanChecklist {
            doc: plan_doc,
            current: plan_current,
            steps,
        })
    }

    pub fn plan_checklist_update(
        &mut self,
        workspace: &WorkspaceId,
        request: PlanChecklistUpdateRequest,
    ) -> Result<(i64, PlanChecklist, EventRow), StoreError> {
        let PlanChecklistUpdateRequest {
            plan_id,
            expected_revision,
            steps,
            current,
            doc,
            advance,
            event_type,
            event_payload_json,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, plan_doc, plan_current
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), &plan_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((revision, current_doc, current_current)) = row else {
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

        if let Some(items) = steps.as_ref() {
            tx.execute(
                "DELETE FROM plan_checklist WHERE workspace = ?1 AND plan_id = ?2",
                params![workspace.as_str(), &plan_id],
            )?;
            for (ordinal, text) in items.iter().enumerate() {
                tx.execute(
                    "INSERT INTO plan_checklist(workspace, plan_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), &plan_id, ordinal as i64, text],
                )?;
            }
        }

        let mut next_current = current.unwrap_or(current_current);
        if advance {
            next_current = next_current.saturating_add(1);
        }

        let next_doc = doc.or(current_doc);
        let new_revision = revision + 1;
        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3,
                plan_doc = ?4,
                plan_current = ?5,
                updated_at_ms = ?6
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                &plan_id,
                new_revision,
                next_doc,
                next_current,
                now_ms
            ],
        )?;

        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &plan_id,
                kind: TaskKind::Plan,
                path: None,
                event_type: &event_type,
                payload_json: &event_payload_json,
            },
        )?;

        tx.commit()?;

        let checklist = self.plan_checklist_get(workspace, &plan_id)?;
        Ok((new_revision, checklist, event))
    }
}
