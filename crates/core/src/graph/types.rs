#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct GraphNode {
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

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub from: String,
    pub rel: String,
    pub to: String,
    pub meta_json: Option<String>,
    pub deleted: bool,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub enum GraphOp {
    NodeUpsert(GraphNodeUpsert),
    NodeDelete {
        id: String,
    },
    EdgeUpsert(GraphEdgeUpsert),
    EdgeDelete {
        from: String,
        rel: String,
        to: String,
    },
}

#[derive(Clone, Debug)]
pub struct GraphNodeUpsert {
    pub id: String,
    pub node_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GraphEdgeUpsert {
    pub from: String,
    pub rel: String,
    pub to: String,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GraphApplyResult {
    pub nodes_upserted: usize,
    pub nodes_deleted: usize,
    pub edges_upserted: usize,
    pub edges_deleted: usize,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphQuerySlice {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphQueryRequest {
    pub ids: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub status: Option<String>,
    pub tags_any: Option<Vec<String>>,
    pub tags_all: Option<Vec<String>>,
    pub text: Option<String>,
    pub cursor: Option<i64>,
    pub limit: usize,
    pub include_edges: bool,
    pub edges_limit: usize,
}

#[derive(Clone, Debug)]
pub struct GraphValidateError {
    pub code: &'static str,
    pub message: String,
    pub kind: &'static str,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct GraphValidateResult {
    pub ok: bool,
    pub nodes: usize,
    pub edges: usize,
    pub errors: Vec<GraphValidateError>,
}

#[derive(Clone, Debug)]
pub enum GraphDiffChange {
    Node { to: GraphNode },
    Edge { to: GraphEdge },
}

#[derive(Clone, Debug)]
pub struct GraphDiffSlice {
    pub changes: Vec<GraphDiffChange>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphMergeResult {
    pub merged: usize,
    pub skipped: usize,
    /// Diverged candidates requiring conflict handling (open/preview conflicts).
    ///
    /// Note: this can be > conflicts_created when conflicts already exist (or when dry_run=true).
    pub conflicts_detected: usize,
    /// New conflict rows inserted into storage.
    pub conflicts_created: usize,
    pub conflict_ids: Vec<String>,
    pub conflicts: Vec<GraphConflictDetail>,
    pub diff_summary: GraphMergeDiffSummary,
    pub count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphMergeDiffSummary {
    pub nodes_changed: usize,
    pub edges_changed: usize,
    pub node_fields_changed: usize,
    pub edge_fields_changed: usize,
}

#[derive(Clone, Debug)]
pub struct GraphConflictSummary {
    pub conflict_id: String,
    pub kind: String,
    pub key: String,
    pub status: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphConflictDetail {
    pub conflict_id: String,
    pub kind: String,
    pub key: String,
    pub from_branch: String,
    pub into_branch: String,
    pub doc: String,
    pub status: String,
    pub created_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
    pub base_node: Option<GraphNode>,
    pub theirs_node: Option<GraphNode>,
    pub ours_node: Option<GraphNode>,
    pub base_edge: Option<GraphEdge>,
    pub theirs_edge: Option<GraphEdge>,
    pub ours_edge: Option<GraphEdge>,
}

#[derive(Clone, Debug)]
pub struct GraphConflictResolveResult {
    pub conflict_id: String,
    pub status: String,
    pub applied: bool,
    pub applied_seq: Option<i64>,
}
