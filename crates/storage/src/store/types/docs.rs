#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Notes,
    Trace,
    Graph,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Trace => "trace",
            Self::Graph => "graph",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocEntryKind {
    Note,
    Event,
}

impl DocEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Event => "event",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DocEntryRow {
    pub seq: i64,
    pub ts_ms: i64,
    pub branch: String,
    pub doc: String,
    pub kind: DocEntryKind,
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

#[derive(Clone, Debug)]
pub struct DocSlice {
    pub entries: Vec<DocEntryRow>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct WorkspaceDocEntryHead {
    pub seq: i64,
    pub ts_ms: i64,
    pub branch: String,
    pub doc: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct DocAppendRequest {
    pub branch: String,
    pub doc: String,
    pub title: Option<String>,
    pub format: Option<String>,
    pub meta_json: Option<String>,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct DocMergeNotesRequest {
    pub from_branch: String,
    pub into_branch: String,
    pub doc: String,
    pub cursor: Option<i64>,
    pub limit: usize,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct DocumentRow {
    pub branch: String,
    pub doc: String,
    pub kind: DocumentKind,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct MergeNotesResult {
    pub merged: usize,
    pub skipped: usize,
    pub count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}
