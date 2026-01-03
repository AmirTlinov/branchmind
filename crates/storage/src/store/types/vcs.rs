#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct VcsRefRow {
    pub reference: String,
    pub branch: String,
    pub doc: String,
    pub seq: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct VcsRefUpdate {
    pub reference: VcsRefRow,
    pub old_seq: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct VcsTagRow {
    pub name: String,
    pub branch: String,
    pub doc: String,
    pub seq: i64,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct VcsReflogRow {
    pub reference: String,
    pub branch: String,
    pub doc: String,
    pub old_seq: Option<i64>,
    pub new_seq: i64,
    pub ts_ms: i64,
    pub message: Option<String>,
}
