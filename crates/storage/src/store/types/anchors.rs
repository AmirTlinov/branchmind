#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct AnchorRow {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub description: Option<String>,
    pub refs: Vec<String>,
    pub aliases: Vec<String>,
    pub parent_id: Option<String>,
    pub depends_on: Vec<String>,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingRow {
    pub kind: String,
    pub repo_rel: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingHit {
    pub anchor_id: String,
    pub kind: String,
    pub repo_rel: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingsLookupAnyRequest {
    pub repo_rels: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingsLookupAnyResult {
    pub bindings: Vec<AnchorBindingHit>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingIndexRow {
    pub anchor_id: String,
    pub anchor_title: Option<String>,
    pub anchor_kind: Option<String>,
    pub kind: String,
    pub repo_rel: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingsIndexListRequest {
    pub prefix: Option<String>,
    pub anchor_id: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorBindingsIndexListResult {
    pub bindings: Vec<AnchorBindingIndexRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorGetRequest {
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct AnchorsListRequest {
    pub text: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorsListResult {
    pub anchors: Vec<AnchorRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorUpsertRequest {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub description: Option<String>,
    pub refs: Vec<String>,
    pub aliases: Vec<String>,
    pub parent_id: Option<String>,
    pub depends_on: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct AnchorUpsertResult {
    pub anchor: AnchorRow,
    pub created: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorRenameRequest {
    pub from_id: String,
    pub to_id: String,
}

#[derive(Clone, Debug)]
pub struct AnchorRenameResult {
    pub from_id: String,
    pub to_id: String,
    pub anchor: AnchorRow,
}

#[derive(Clone, Debug)]
pub struct AnchorsBootstrapRequest {
    pub anchors: Vec<AnchorUpsertRequest>,
}

#[derive(Clone, Debug)]
pub struct AnchorsBootstrapResult {
    pub anchors: Vec<AnchorUpsertResult>,
}

#[derive(Clone, Debug)]
pub struct AnchorsMergeRequest {
    pub into_id: String,
    pub from_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AnchorsMergeResult {
    pub into_id: String,
    pub from_ids: Vec<String>,
    pub merged_ids: Vec<String>,
    pub skipped_ids: Vec<String>,
    pub anchor: AnchorRow,
}

#[derive(Clone, Debug)]
pub struct AnchorsLintRequest {
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorsLintIssue {
    pub code: String,
    pub severity: String,
    pub anchor: String,
    pub message: String,
    pub hint: String,
}

#[derive(Clone, Debug)]
pub struct AnchorsLintResult {
    pub issues: Vec<AnchorsLintIssue>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorLinkRow {
    pub anchor_id: String,
    pub branch: String,
    pub graph_doc: String,
    pub card_id: String,
    pub card_type: String,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorTaskHit {
    pub task_id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct AnchorTasksListAnyRequest {
    pub anchor_ids: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorTasksListResult {
    pub tasks: Vec<AnchorTaskHit>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorLinksListRequest {
    pub anchor_id: String,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct AnchorLinksListResult {
    pub links: Vec<AnchorLinkRow>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct AnchorLinksListAnyRequest {
    pub anchor_ids: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct TaskAnchorHit {
    pub anchor_id: String,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct TaskAnchorsListRequest {
    pub task_id: String,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct TaskAnchorsListResult {
    pub anchors: Vec<TaskAnchorHit>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct PlanAnchorHit {
    pub anchor_id: String,
    pub last_ts_ms: i64,
    pub task_count: u64,
}

#[derive(Clone, Debug)]
pub struct PlanAnchorsCoverageRequest {
    pub plan_id: String,
    pub top_anchors_limit: usize,
}

#[derive(Clone, Debug)]
pub struct PlanAnchorsCoverageResult {
    pub active_total: u64,
    pub active_missing_anchor: u64,
    pub top_anchors: Vec<PlanAnchorHit>,
}
