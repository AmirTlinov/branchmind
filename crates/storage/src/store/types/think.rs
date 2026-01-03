#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct ThinkCardInput {
    pub card_id: String,
    pub card_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub meta_json: Option<String>,
    pub content: String,
    pub payload_json: String,
}

#[derive(Clone, Debug)]
pub struct ThinkCardCommitResult {
    pub inserted: bool,
    pub nodes_upserted: usize,
    pub edges_upserted: usize,
    pub last_seq: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ThinkCardCommitRequest {
    pub branch: String,
    pub trace_doc: String,
    pub graph_doc: String,
    pub card: ThinkCardInput,
    pub supports: Vec<String>,
    pub blocks: Vec<String>,
}
