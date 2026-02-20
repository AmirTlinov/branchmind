#![forbid(unsafe_code)]

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateBranchRequest {
    pub workspace_id: String,
    pub branch_id: String,
    pub parent_branch_id: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListBranchesRequest {
    pub workspace_id: String,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteBranchRequest {
    pub workspace_id: String,
    pub branch_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendCommitRequest {
    pub workspace_id: String,
    pub branch_id: String,
    pub commit_id: String,
    pub parent_commit_id: Option<String>,
    pub message: String,
    pub body: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowCommitRequest {
    pub workspace_id: String,
    pub commit_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateMergeRecordRequest {
    pub workspace_id: String,
    pub merge_id: String,
    pub source_branch_id: String,
    pub target_branch_id: String,
    pub strategy: String,
    pub summary: String,
    pub synthesis_commit_id: String,
    pub synthesis_message: String,
    pub synthesis_body: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListMergeRecordsRequest {
    pub workspace_id: String,
    pub limit: usize,
    pub offset: usize,
}
