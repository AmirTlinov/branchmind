#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct WorkspaceRow {
    pub workspace: String,
    pub created_at_ms: i64,
    pub project_guard: Option<String>,
}
