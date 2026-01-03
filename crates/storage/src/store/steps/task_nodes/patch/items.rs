#![forbid(unsafe_code)]

use super::super::super::super::*;
use super::presence::PatchPresence;
use rusqlite::Transaction;

pub(super) struct ItemsPatch {
    pub(super) blockers: Option<Vec<String>>,
    pub(super) dependencies: Option<Vec<String>>,
    pub(super) next_steps: Option<Vec<String>>,
    pub(super) problems: Option<Vec<String>>,
    pub(super) risks: Option<Vec<String>>,
    pub(super) success_criteria: Option<Vec<String>>,
}

impl ItemsPatch {
    pub(super) fn apply(self, before: &TaskNodeItems) -> TaskNodeItems {
        TaskNodeItems {
            blockers: self.blockers.unwrap_or_else(|| before.blockers.clone()),
            dependencies: self
                .dependencies
                .unwrap_or_else(|| before.dependencies.clone()),
            next_steps: self.next_steps.unwrap_or_else(|| before.next_steps.clone()),
            problems: self.problems.unwrap_or_else(|| before.problems.clone()),
            risks: self.risks.unwrap_or_else(|| before.risks.clone()),
            success_criteria: self
                .success_criteria
                .unwrap_or_else(|| before.success_criteria.clone()),
        }
    }
}

pub(super) fn load_task_node_items_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    node_id: &str,
) -> Result<TaskNodeItems, StoreError> {
    Ok(TaskNodeItems {
        blockers: task_items_list_tx(tx, workspace, "task_node", node_id, "blockers")?,
        dependencies: task_items_list_tx(tx, workspace, "task_node", node_id, "dependencies")?,
        next_steps: task_items_list_tx(tx, workspace, "task_node", node_id, "next_steps")?,
        problems: task_items_list_tx(tx, workspace, "task_node", node_id, "problems")?,
        risks: task_items_list_tx(tx, workspace, "task_node", node_id, "risks")?,
        success_criteria: task_items_list_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "success_criteria",
        )?,
    })
}

pub(super) fn replace_task_node_items_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    node_id: &str,
    presence: &PatchPresence,
    next: &TaskNodeItems,
) -> Result<(), StoreError> {
    if presence.blockers {
        task_items_replace_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "blockers",
            &next.blockers,
        )?;
    }
    if presence.dependencies {
        task_items_replace_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "dependencies",
            &next.dependencies,
        )?;
    }
    if presence.next_steps {
        task_items_replace_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "next_steps",
            &next.next_steps,
        )?;
    }
    if presence.problems {
        task_items_replace_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "problems",
            &next.problems,
        )?;
    }
    if presence.risks {
        task_items_replace_tx(tx, workspace, "task_node", node_id, "risks", &next.risks)?;
    }
    if presence.success_criteria {
        task_items_replace_tx(
            tx,
            workspace,
            "task_node",
            node_id,
            "success_criteria",
            &next.success_criteria,
        )?;
    }
    Ok(())
}
