#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;

impl SqliteStore {
    pub fn create(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskCreateRequest,
    ) -> Result<(String, i64, EventRow), StoreError> {
        let TaskCreateRequest {
            kind,
            title,
            parent_plan_id,
            description,
            contract,
            contract_json,
            event_type,
            event_payload_json,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let id = match kind {
            TaskKind::Plan => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "plan_seq")?;
                format!("PLAN-{:03}", seq)
            }
            TaskKind::Task => {
                let seq = next_counter_tx(&tx, workspace.as_str(), "task_seq")?;
                format!("TASK-{:03}", seq)
            }
        };

        match kind {
            TaskKind::Plan => {
                tx.execute(
                    r#"
                    INSERT INTO plans(workspace,id,revision,title,contract,contract_json,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        title,
                        contract,
                        contract_json,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
            TaskKind::Task => {
                let parent_plan_id = parent_plan_id
                    .ok_or(StoreError::InvalidInput("parent is required for kind=task"))?;
                tx.execute(
                    r#"
                    INSERT INTO tasks(workspace,id,revision,parent_plan_id,title,description,created_at_ms,updated_at_ms)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                    "#,
                    params![
                        workspace.as_str(),
                        id,
                        0i64,
                        parent_plan_id,
                        title,
                        description,
                        now_ms,
                        now_ms
                    ],
                )?;
            }
        }

        let event = insert_event_tx(
            &tx,
            workspace.as_str(),
            now_ms,
            match kind {
                TaskKind::Plan => Some(id.clone()),
                TaskKind::Task => Some(id.clone()),
            },
            None,
            &event_type,
            &event_payload_json,
        )?;

        let reasoning_ref = ensure_reasoning_ref_tx(&tx, workspace, &id, kind, now_ms)?;
        let _ = ingest_task_event_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref.branch,
            &reasoning_ref.trace_doc,
            &event,
        )?;

        if matches!(kind, TaskKind::Task) {
            let touched = Self::project_task_graph_task_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &id,
                &title,
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
        }

        tx.commit()?;
        Ok((id, 0i64, event))
    }
}
