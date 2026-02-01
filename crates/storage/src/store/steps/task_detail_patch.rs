#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, params};
use serde_json::json;

impl SqliteStore {
    pub fn task_detail_patch(
        &mut self,
        workspace: &WorkspaceId,
        request: TaskDetailPatchRequest,
    ) -> Result<(i64, EventRow), StoreError> {
        let TaskDetailPatchRequest {
            task_id,
            expected_revision,
            kind,
            patch,
            event_type,
            event_payload_json,
            record_undo,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        let (
            mut title,
            mut description,
            mut context,
            mut priority,
            mut contract,
            mut contract_json,
        );
        let (mut domain, mut phase, mut component, mut assignee);

        match kind {
            TaskKind::Plan => {
                let row = tx
                    .query_row(
                        "SELECT title, description, context, priority, contract, contract_json FROM plans WHERE workspace=?1 AND id=?2",
                        params![workspace.as_str(), task_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, Option<String>>(5)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((t, d, c, p, ct, cj)) = row else {
                    return Err(StoreError::UnknownId);
                };
                title = t;
                description = d;
                context = c;
                priority = p;
                contract = ct;
                contract_json = cj;
                domain = None;
                phase = None;
                component = None;
                assignee = None;
            }
            TaskKind::Task => {
                let row = tx
                    .query_row(
                        "SELECT title, description, context, priority, domain, phase, component, assignee FROM tasks WHERE workspace=?1 AND id=?2",
                        params![workspace.as_str(), task_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, Option<String>>(5)?,
                                row.get::<_, Option<String>>(6)?,
                                row.get::<_, Option<String>>(7)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((t, d, c, p, dm, ph, comp, asg)) = row else {
                    return Err(StoreError::UnknownId);
                };
                title = t;
                description = d;
                context = c;
                priority = p;
                domain = dm;
                phase = ph;
                component = comp;
                assignee = asg;
                contract = None;
                contract_json = None;
            }
        }

        let tags = task_items_list_tx(&tx, workspace.as_str(), kind.as_str(), &task_id, "tags")?;
        let depends_on = task_items_list_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            &task_id,
            "depends_on",
        )?;

        let before_snapshot = json!({
            "kind": kind.as_str(),
            "task": task_id.as_str(),
            "title": title,
            "description": description,
            "context": context,
            "priority": priority,
            "contract": contract,
            "contract_data": parse_json_or_null(contract_json.clone()),
            "domain": domain,
            "phase": phase,
            "component": component,
            "assignee": assignee,
            "tags": tags,
            "depends_on": depends_on
        });

        if let Some(v) = patch.title {
            title = v;
        }
        if let Some(v) = patch.description {
            description = v;
        }
        if let Some(v) = patch.context {
            context = v;
        }
        if let Some(v) = patch.priority {
            priority = v;
        }
        if let Some(v) = patch.contract {
            contract = v;
        }
        if let Some(v) = patch.contract_json {
            contract_json = v;
        }
        if let Some(v) = patch.domain {
            domain = v;
        }
        if let Some(v) = patch.phase {
            phase = v;
        }
        if let Some(v) = patch.component {
            component = v;
        }
        if let Some(v) = patch.assignee {
            assignee = v;
        }
        let next_tags = patch.tags.unwrap_or_else(|| tags.clone());
        let next_depends = patch.depends_on.unwrap_or_else(|| depends_on.clone());

        let revision = match kind {
            TaskKind::Plan => {
                bump_plan_revision_tx(&tx, workspace.as_str(), &task_id, expected_revision, now_ms)?
            }
            TaskKind::Task => {
                bump_task_revision_tx(&tx, workspace.as_str(), &task_id, expected_revision, now_ms)?
            }
        };

        match kind {
            TaskKind::Plan => {
                tx.execute(
                    r#"
                    UPDATE plans
                    SET title=?3, description=?4, context=?5, priority=?6, contract=?7, contract_json=?8, updated_at_ms=?9
                    WHERE workspace=?1 AND id=?2
                    "#,
                    params![
                        workspace.as_str(),
                        task_id,
                        title,
                        description,
                        context,
                        priority,
                        contract,
                        contract_json,
                        now_ms
                    ],
                )?;
            }
            TaskKind::Task => {
                tx.execute(
                    r#"
                    UPDATE tasks
                    SET title=?3, description=?4, context=?5, priority=?6,
                        domain=?7, phase=?8, component=?9, assignee=?10, updated_at_ms=?11
                    WHERE workspace=?1 AND id=?2
                    "#,
                    params![
                        workspace.as_str(),
                        task_id,
                        title,
                        description,
                        context,
                        priority,
                        domain,
                        phase,
                        component,
                        assignee,
                        now_ms
                    ],
                )?;
            }
        }

        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            &task_id,
            "tags",
            &next_tags,
        )?;
        task_items_replace_tx(
            &tx,
            workspace.as_str(),
            kind.as_str(),
            &task_id,
            "depends_on",
            &next_depends,
        )?;

        let after_snapshot = json!({
            "kind": kind.as_str(),
            "task": task_id.as_str(),
            "title": title,
            "description": description,
            "context": context,
            "priority": priority,
            "contract": contract,
            "contract_data": parse_json_or_null(contract_json.clone()),
            "domain": domain,
            "phase": phase,
            "component": component,
            "assignee": assignee,
            "tags": next_tags,
            "depends_on": next_depends
        });

        let (event, reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind,
                path: None,
                event_type: &event_type,
                payload_json: &event_payload_json,
            },
        )?;

        if record_undo {
            ops_history_insert_tx(
                &tx,
                OpsHistoryInsertTxArgs {
                    workspace: workspace.as_str(),
                    task_id: Some(task_id.as_str()),
                    path: None,
                    intent: "task_detail_patch",
                    payload_json: &event_payload_json,
                    before_json: Some(&before_snapshot.to_string()),
                    after_json: Some(&after_snapshot.to_string()),
                    undoable: true,
                    now_ms,
                },
            )?;
        }

        if matches!(kind, TaskKind::Task) {
            let touched = Self::project_task_graph_task_node_tx(
                &tx,
                workspace.as_str(),
                &reasoning_ref,
                &event,
                &task_id,
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
        Ok((revision, event))
    }
}
