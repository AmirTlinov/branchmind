#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub id: String,
    pub revision: i64,
    pub parent_plan_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub assignee: Option<String>,
    pub domain: Option<String>,
    pub phase: Option<String>,
    pub component: Option<String>,
    pub context: Option<String>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub criteria_auto_confirmed: bool,
    pub tests_auto_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct TaskCreateRequest {
    pub kind: bm_core::model::TaskKind,
    pub title: String,
    pub parent_plan_id: Option<String>,
    pub description: Option<String>,
    pub contract: Option<String>,
    pub contract_json: Option<String>,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct TaskEditRequest {
    pub id: String,
    pub expected_revision: Option<i64>,
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub priority: Option<String>,
    pub domain: Option<Option<String>>,
    pub phase: Option<Option<String>>,
    pub component: Option<Option<String>>,
    pub assignee: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct SetTaskStatusRequest {
    pub id: String,
    pub expected_revision: Option<i64>,
    pub status: String,
    pub status_manual: bool,
    pub require_steps_completed: bool,
    pub event_type: String,
    pub event_payload_json: String,
}

#[derive(Clone, Debug)]
pub struct TaskDetailPatch {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub priority: Option<String>,
    pub contract: Option<Option<String>>,
    pub contract_json: Option<Option<String>>,
    pub domain: Option<Option<String>>,
    pub phase: Option<Option<String>>,
    pub component: Option<Option<String>>,
    pub assignee: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct TaskDetailPatchRequest {
    pub task_id: String,
    pub expected_revision: Option<i64>,
    pub kind: bm_core::model::TaskKind,
    pub patch: TaskDetailPatch,
    pub event_type: String,
    pub event_payload_json: String,
    pub record_undo: bool,
}
