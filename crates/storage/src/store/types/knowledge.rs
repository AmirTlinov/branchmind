#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct KnowledgeKeyRow {
    pub anchor_id: String,
    pub key: String,
    pub card_id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct KnowledgeKeysListAnyRequest {
    pub anchor_ids: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct KnowledgeKeysListByKeyRequest {
    /// Knowledge key slug (`<slug>`) or tag (`k:<slug>`).
    pub key: String,
    pub anchor_ids: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct KnowledgeKeysListResult {
    pub items: Vec<KnowledgeKeyRow>,
    pub has_more: bool,
}
