#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn edit_task(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskEditRequest,
    ) -> Result<(i64, EventRow), StoreError> {
        let TaskEditRequest {
            id,
            expected_revision,
            title,
            description,
            context,
            priority,
            domain,
            reasoning_mode,
            phase,
            component,
            assignee,
            tags,
            depends_on,
            event_type,
            event_payload_json,
        } = request;

        if title.is_none()
            && description.is_none()
            && context.is_none()
            && priority.is_none()
            && domain.is_none()
            && reasoning_mode.is_none()
            && phase.is_none()
            && component.is_none()
            && assignee.is_none()
            && tags.is_none()
            && depends_on.is_none()
        {
            return Err(StoreError::InvalidInput("no fields to edit"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = tx
            .query_row(
                r#"
                SELECT revision, title, description, context, priority, domain, reasoning_mode, phase, component, assignee
                FROM tasks
                WHERE workspace = ?1 AND id = ?2
                "#,
                params![workspace.as_str(), &id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            revision,
            current_title,
            current_description,
            current_context,
            current_priority,
            current_domain,
            current_reasoning_mode,
            current_phase,
            current_component,
            current_assignee,
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
        let new_description = description.unwrap_or(current_description);
        let new_context = context.unwrap_or(current_context);
        let new_priority = priority.unwrap_or(current_priority);
        let new_domain = domain.unwrap_or(current_domain);
        let new_reasoning_mode = reasoning_mode.unwrap_or(current_reasoning_mode);
        let new_phase = phase.unwrap_or(current_phase);
        let new_component = component.unwrap_or(current_component);
        let new_assignee = assignee.unwrap_or(current_assignee);

        tx.execute(
            r#"
            UPDATE tasks
            SET revision = ?3,
                title = ?4,
                description = ?5,
                context = ?6,
                priority = ?7,
                domain = ?8,
                reasoning_mode = ?9,
                phase = ?10,
                component = ?11,
                assignee = ?12,
                updated_at_ms = ?13
            WHERE workspace = ?1 AND id = ?2
            "#,
            params![
                workspace.as_str(),
                &id,
                new_revision,
                new_title,
                new_description,
                new_context,
                new_priority,
                new_domain,
                new_reasoning_mode,
                new_phase,
                new_component,
                new_assignee,
                now_ms
            ],
        )?;
        if let Some(items) = tags {
            task_items_replace_tx(&tx, workspace.as_str(), "task", &id, "tags", &items)?;
        }
        if let Some(items) = depends_on {
            task_items_replace_tx(&tx, workspace.as_str(), "task", &id, "depends_on", &items)?;
        }

        let (event, reasoning_ref) = emit_task_event_tx(
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

        let touched = Self::project_task_graph_task_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            &id,
            &new_title,
            now_ms,
        )?;
        if touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok((new_revision, event))
    }
}
