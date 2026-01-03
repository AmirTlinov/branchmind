#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub base_branch: Option<String>,
    pub base_seq: Option<i64>,
    pub created_at_ms: Option<i64>,
}
