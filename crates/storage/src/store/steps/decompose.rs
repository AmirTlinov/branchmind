#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use rusqlite::{OptionalExtension, params};

impl SqliteStore {
    pub fn steps_decompose(
        &mut self,
        workspace: &WorkspaceId,
        task_id: &str,
        expected_revision: Option<i64>,
        parent_path: Option<&StepPath>,
        steps: Vec<NewStep>,
    ) -> Result<DecomposeResult, StoreError> {
        if steps.is_empty() {
            return Err(StoreError::InvalidInput("steps must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), task_id, expected_revision, now_ms)?;

        let parent_step_id = match parent_path {
            None => None,
            Some(path) => Some(resolve_step_id_tx(&tx, workspace.as_str(), task_id, path)?),
        };

        let max_ordinal: Option<i64> = match parent_step_id.as_deref() {
            None => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id IS NULL",
                    params![workspace.as_str(), task_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
            Some(parent_step_id) => tx
                .query_row(
                    "SELECT MAX(ordinal) FROM steps WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
                    params![workspace.as_str(), task_id, parent_step_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten(),
        };

        let mut next_ordinal = max_ordinal.unwrap_or(-1) + 1;
        let mut created_steps = Vec::with_capacity(steps.len());

        for step in steps {
            let seq = next_counter_tx(&tx, workspace.as_str(), "step_seq")?;
            let step_id = format!("STEP-{seq:08X}");
            let ordinal = next_ordinal;
            next_ordinal += 1;

            tx.execute(
                r#"
                INSERT INTO steps(workspace,task_id,step_id,parent_step_id,ordinal,title,completed,criteria_confirmed,tests_confirmed,created_at_ms,updated_at_ms)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                "#,
                params![
                    workspace.as_str(),
                    task_id,
                    step_id,
                    parent_step_id,
                    ordinal,
                    step.title,
                    0i64,
                    0i64,
                    0i64,
                    now_ms,
                    now_ms
                ],
            )?;

            for (i, text) in step.success_criteria.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO step_criteria(workspace, step_id, ordinal, text) VALUES (?1, ?2, ?3, ?4)",
                    params![workspace.as_str(), step_id, i as i64, text],
                )?;
            }

            let path = match parent_path {
                None => StepPath::root(ordinal as usize).to_string(),
                Some(parent) => parent.child(ordinal as usize).to_string(),
            };
            created_steps.push(StepRef { step_id, path });
        }

        let parent_path_str = parent_path.map(|p| p.to_string());
        let event_payload_json =
            build_steps_added_payload(task_id, parent_path_str.as_deref(), &created_steps);
        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id,
                kind: TaskKind::Task,
                path: parent_path_str,
                event_type: "steps_added",
                payload_json: &event_payload_json,
            },
        )?;

        let mut graph_touched = false;
        let task_title = task_title_tx(&tx, workspace.as_str(), task_id)?;
        graph_touched |= Self::project_task_graph_task_node_tx(
            &tx,
            workspace.as_str(),
            &reasoning_ref,
            &event,
            task_id,
            &task_title,
            now_ms,
        )?;

        let parent_node_id = if let Some(parent_step_id) = parent_step_id.clone() {
            let parent_path = parent_path
                .map(|p| p.to_string())
                .unwrap_or_else(|| "s:?".to_string());
            let parent_ref = StepRef {
                step_id: parent_step_id,
                path: parent_path,
            };
            let (parent_title, parent_completed) =
                step_snapshot_tx(&tx, workspace.as_str(), task_id, &parent_ref.step_id)?;
            graph_touched |= Self::project_task_graph_step_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                task_id,
                &parent_ref,
                &parent_title,
                parent_completed,
                now_ms,
            )?;
            step_graph_node_id(&parent_ref.step_id)
        } else {
            task_graph_node_id(task_id)
        };

        for step in created_steps.iter() {
            let (title, completed) =
                step_snapshot_tx(&tx, workspace.as_str(), task_id, &step.step_id)?;
            graph_touched |= Self::project_task_graph_step_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                task_id,
                step,
                &title,
                completed,
                now_ms,
            )?;

            let step_node_id = step_graph_node_id(&step.step_id);
            graph_touched |= Self::project_task_graph_contains_edge_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &parent_node_id,
                &step_node_id,
                now_ms,
            )?;
        }
        if graph_touched {
            touch_document_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref.branch,
                &reasoning_ref.graph_doc,
                now_ms,
            )?;
        }

        tx.commit()?;
        Ok(DecomposeResult {
            task_revision,
            steps: created_steps,
            event,
        })
    }
}
