#![forbid(unsafe_code)]

use super::super::super::super::TaskNodeItems;
use super::node_row::TaskNodeRowFields;
use serde_json::{Value, json};

pub(super) fn task_node_snapshot_json(
    task_id: &str,
    node_id: &str,
    path: &str,
    fields: &TaskNodeRowFields,
    items: &TaskNodeItems,
) -> Value {
    json!({
        "task": task_id,
        "node_id": node_id,
        "path": path,
        "title": &fields.title,
        "status": &fields.status,
        "status_manual": fields.status_manual,
        "priority": &fields.priority,
        "blocked": fields.blocked,
        "description": &fields.description,
        "context": &fields.context,
        "blockers": &items.blockers,
        "dependencies": &items.dependencies,
        "next_steps": &items.next_steps,
        "problems": &items.problems,
        "risks": &items.risks,
        "success_criteria": &items.success_criteria
    })
}
