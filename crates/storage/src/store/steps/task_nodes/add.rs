#![forbid(unsafe_code)]

use super::super::super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::params;
use serde_json::json;

impl SqliteStore {
    pub fn task_node_add(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskNodeAddRequest,
    ) -> Result<TaskNodeOpResult, StoreError> {
        let TaskNodeAddRequest {
            task_id,
            expected_revision,
            parent_path,
            title,
            status,
            status_manual,
            priority,
            blocked,
            description,
            context,
            items,
            record_undo,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let task_revision =
            bump_task_revision_tx(&tx, workspace.as_str(), &task_id, expected_revision, now_ms)?;
        let parent_step_id = resolve_step_id_tx(&tx, workspace.as_str(), &task_id, &parent_path)?;
        let ordinal: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal), -1) FROM task_nodes WHERE workspace=?1 AND task_id=?2 AND parent_step_id=?3",
            params![workspace.as_str(), task_id, parent_step_id],
            |row| row.get(0),
        )?;
        let ordinal = ordinal + 1;
        let seq = next_counter_tx(&tx, workspace.as_str(), "task_node_seq")?;
        let node_id = format!("NODE-{seq:08X}");

        tx.execute(
            r#"
            INSERT INTO task_nodes(
                workspace, node_id, task_id, parent_step_id, ordinal,
                title, status, status_manual, priority, blocked, description, context,
                created_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                workspace.as_str(),
                node_id,
                task_id,
                parent_step_id,
                ordinal,
                title,
                status,
                if status_manual { 1i64 } else { 0i64 },
                priority,
                if blocked { 1i64 } else { 0i64 },
                description,
                context,
                now_ms,
                now_ms
            ],
        )?;

        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "blockers",
            &items.blockers,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "dependencies",
            &items.dependencies,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "next_steps",
            &items.next_steps,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "problems",
            &items.problems,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "risks",
            &items.risks,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            "task_node",
            &node_id,
            "success_criteria",
            &items.success_criteria,
        )?;

        let path = task_node_path_for_parent_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            &parent_step_id,
            ordinal,
        )?;
        let event_payload_json =
            build_task_node_added_payload(&task_id, &node_id, &path, &parent_path.to_string());
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "task_node_added",
                payload_json: &event_payload_json,
            },
        )?;

        if record_undo {
            let after_snapshot = json!({
                "task": task_id.as_str(),
                "node_id": node_id.clone(),
                "path": path.clone(),
                "title": title,
                "status": status,
                "status_manual": status_manual,
                "priority": priority,
                "blocked": blocked,
                "description": description,
                "context": context,
                "blockers": items.blockers,
                "dependencies": items.dependencies,
                "next_steps": items.next_steps,
                "problems": items.problems,
                "risks": items.risks,
                "success_criteria": items.success_criteria
            });
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: Some(path.clone()),
                    intent: "task_node_add",
                    payload_json: &event_payload_json,
                    before_json: None,
                    after_json: Some(&after_snapshot.to_string()),
                    undoable: false,
                    now_ms,
                },
            )?;
        }

        tx.commit()?;
        Ok(TaskNodeOpResult {
            task_revision,
            node: TaskNodeRef { node_id, path },
            event,
        })
    }
}
