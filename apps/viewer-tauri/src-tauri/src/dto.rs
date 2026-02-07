#![forbid(unsafe_code)]

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceDto {
    pub workspace: String,
    pub created_at_ms: i64,
    pub project_guard: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectDto {
    pub project_id: String,
    pub display_name: String,
    pub storage_dir: String,
    pub db_path: String,
    pub repo_root: Option<String>,
    pub workspaces: Vec<WorkspaceDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskSummaryDto {
    pub id: String,
    pub parent_plan_id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub blocked: bool,
    pub reasoning_mode: String,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskDto {
    pub id: String,
    pub revision: i64,
    pub parent_plan_id: String,
    pub title: String,
    pub description: Option<String>,
    pub context: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub blocked: bool,
    pub assignee: Option<String>,
    pub domain: Option<String>,
    pub phase: Option<String>,
    pub component: Option<String>,
    pub reasoning_mode: String,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanDto {
    pub id: String,
    pub revision: i64,
    pub title: String,
    pub description: Option<String>,
    pub context: Option<String>,
    pub status: String,
    pub status_manual: bool,
    pub priority: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepListDto {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub completed: bool,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepDetailDto {
    pub step_id: String,
    pub path: String,
    pub title: String,
    pub next_action: Option<String>,
    pub stop_criteria: Option<String>,
    pub success_criteria: Vec<String>,
    pub tests: Vec<String>,
    pub blockers: Vec<String>,
    pub criteria_confirmed: bool,
    pub tests_confirmed: bool,
    pub security_confirmed: bool,
    pub perf_confirmed: bool,
    pub docs_confirmed: bool,
    pub completed: bool,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub proof_tests_mode: String,
    pub proof_security_mode: String,
    pub proof_perf_mode: String,
    pub proof_docs_mode: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskStepsSummaryDto {
    pub total_steps: i64,
    pub completed_steps: i64,
    pub open_steps: i64,
    pub missing_criteria: i64,
    pub missing_tests: i64,
    pub missing_security: i64,
    pub missing_perf: i64,
    pub missing_docs: i64,
    pub first_open: Option<StepDetailDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReasoningRefDto {
    pub branch: String,
    pub notes_doc: String,
    pub graph_doc: String,
    pub trace_doc: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocEntryDto {
    pub seq: i64,
    pub ts_ms: i64,
    pub branch: String,
    pub doc: String,
    pub kind: String,
    pub title: Option<String>,
    pub format: Option<String>,
    pub meta_json: Option<String>,
    pub content: Option<String>,
    pub source_event_id: Option<String>,
    pub event_type: Option<String>,
    pub task_id: Option<String>,
    pub path: Option<String>,
    pub payload_json: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocEntriesSinceDto {
    pub entries: Vec<DocEntryDto>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocSliceDto {
    pub entries: Vec<DocEntryDto>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BranchDto {
    pub name: String,
    pub base_branch: Option<String>,
    pub base_seq: Option<i64>,
    pub created_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphNodeDto {
    pub id: String,
    pub node_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub meta_json: Option<String>,
    pub deleted: bool,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphEdgeDto {
    pub from: String,
    pub rel: String,
    pub to: String,
    pub meta_json: Option<String>,
    pub deleted: bool,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphSliceDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphDiffChangeDto {
    Node { to: GraphNodeDto },
    Edge { to: GraphEdgeDto },
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphDiffSliceDto {
    pub changes: Vec<GraphDiffChangeDto>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct KnowledgeKeyDto {
    pub anchor_id: String,
    pub key: String,
    pub card_id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct KnowledgeSearchDto {
    pub items: Vec<KnowledgeKeyDto>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskSearchHitDto {
    pub id: String,
    pub plan_id: String,
    pub title: String,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TasksSearchDto {
    pub tasks: Vec<TaskSearchHitDto>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnchorDto {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub parent_id: Option<String>,
    pub refs: Vec<String>,
    pub depends_on: Vec<String>,
    pub aliases: Vec<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnchorsListDto {
    pub anchors: Vec<AnchorDto>,
    pub has_more: bool,
}
