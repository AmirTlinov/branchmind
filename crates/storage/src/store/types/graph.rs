#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct GraphMergeBackRequest {
    pub from_branch: String,
    pub into_branch: String,
    pub doc: String,
    pub cursor: Option<i64>,
    pub limit: usize,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct GraphCardOpenHead {
    pub branch: String,
    pub doc: String,
    pub seq: i64,
    pub ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphCardOpenResult {
    pub head: GraphCardOpenHead,
    pub node: GraphNodeRow,
    pub supports: Vec<String>,
    pub blocks: Vec<String>,
}

pub use bm_core::graph::{
    GraphApplyResult, GraphConflictDetail, GraphConflictResolveResult, GraphConflictSummary,
    GraphDiffChange, GraphDiffSlice, GraphEdge, GraphEdgeUpsert, GraphMergeDiffSummary,
    GraphMergeResult, GraphNode, GraphNodeUpsert, GraphOp, GraphQueryRequest, GraphQuerySlice,
    GraphValidateError, GraphValidateResult,
};

pub type GraphNodeRow = GraphNode;
pub type GraphEdgeRow = GraphEdge;
