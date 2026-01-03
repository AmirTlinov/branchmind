#![forbid(unsafe_code)]

use super::events::EventRow;

#[derive(Clone, Debug)]
pub struct TaskNodePatch {
    pub title: Option<String>,
    pub status: Option<String>,
    pub status_manual: Option<bool>,
    pub priority: Option<String>,
    pub blocked: Option<bool>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub blockers: Option<Vec<String>>,
    pub dependencies: Option<Vec<String>>,
    pub next_steps: Option<Vec<String>>,
    pub problems: Option<Vec<String>>,
    pub risks: Option<Vec<String>>,
    pub success_criteria: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct TaskNodeItems {
    pub blockers: Vec<String>,
    pub dependencies: Vec<String>,
    pub next_steps: Vec<String>,
    pub problems: Vec<String>,
    pub risks: Vec<String>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskNodeAddRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub parent_path: bm_core::paths::StepPath,
    pub title: String,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub description: Option<String>,
    pub context: Option<String>,
    pub items: TaskNodeItems,
    pub record_undo: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TaskNodeSelector {
    pub node_id: Option<String>,
    pub parent_path: Option<bm_core::paths::StepPath>,
    pub ordinal: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct TaskNodePatchRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub selector: TaskNodeSelector,
    pub patch: TaskNodePatch,
    pub record_undo: bool,
}

#[derive(Clone, Debug)]
pub struct TaskNodeDeleteRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub selector: TaskNodeSelector,
    pub record_undo: bool,
}

#[derive(Clone, Debug)]
pub struct TaskNodeRow {
    pub node_id: String,
    pub task_id: String,
    pub parent_step_id: String,
    pub ordinal: i64,
    pub title: String,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub description: Option<String>,
    pub context: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct TaskNodeDetail {
    pub row: TaskNodeRow,
    pub path: String,
    pub blockers: Vec<String>,
    pub dependencies: Vec<String>,
    pub next_steps: Vec<String>,
    pub problems: Vec<String>,
    pub risks: Vec<String>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskNodeRef {
    pub node_id: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct TaskNodeOpResult {
    pub task_revision: i64,
    pub node: TaskNodeRef,
    pub event: EventRow,
}
