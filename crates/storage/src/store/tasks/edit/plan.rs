#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn edit_plan(
        &mut self,
        workspace: &WorkspaceId,
        request: PlanEditRequest,
    ) -> Result<(i64, EventRow), StoreError> {
        let PlanEditRequest {
            id,
            expected_revision,
            title,
            description,
            context,
            priority,
            tags,
            depends_on,
            contract,
            contract_json,
            event_type,
            event_payload_json,
        } = request;

        if title.is_none()
            && description.is_none()
            && context.is_none()
            && priority.is_none()
            && tags.is_none()
            && depends_on.is_none()
            && contract.is_none()
            && contract_json.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, contract, contract_json, description, context, priority
                FROM plans
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), &id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            revision,
            current_title,
            current_contract,
            current_contract_json,
            current_description,
            current_context,
            current_priority,
        )) = row
        else {
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
        let new_title = title.unwrap_or(current_title);
        let new_contract = contract.unwrap_or(current_contract);
        let new_contract_json = contract_json.unwrap_or(current_contract_json);
        let new_description = description.unwrap_or(current_description);
        let new_context = context.unwrap_or(current_context);
        let new_priority = priority.unwrap_or(current_priority);

        tx.execute(
            r#"
            UPDATE plans
            SET revision = ?3,
                title = ?4,
                contract = ?5,
                contract_json = ?6,
                description = ?7,
                context = ?8,
                priority = ?9,
                updated_at_ms = ?10
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                &id,
                new_revision,
                new_title,
                new_contract,
                new_contract_json,
                new_description,
                new_context,
                new_priority,
                now_ms
            ],
        )?;
        if let Some(items) = tags {
            task_items_replace_tx(&tx, workspace.as_str(), "plan", &id, "tags", &items)?;
        }
        if let Some(items) = depends_on {
            task_items_replace_tx(&tx, workspace.as_str(), "plan", &id, "depends_on", &items)?;
        }

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
}
