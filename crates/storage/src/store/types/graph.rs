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

pub use bm_core::graph::{
    GraphApplyResult, GraphConflictDetail, GraphConflictResolveResult, GraphConflictSummary,
    GraphDiffChange, GraphDiffSlice, GraphEdge, GraphEdgeUpsert, GraphMergeDiffSummary,
    GraphMergeResult, GraphNode, GraphNodeUpsert, GraphOp, GraphQueryRequest, GraphQuerySlice,
    GraphValidateError, GraphValidateResult,
};

pub type GraphNodeRow = GraphNode;
pub type GraphEdgeRow = GraphEdge;
