#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) anchor_id: String,
    pub(crate) key: String,
    pub(crate) card_id: String,
    pub(crate) created_at_ms: i64,
    pub(crate) content_hash: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct DuplicateGroup {
    pub(crate) anchor_id: String,
    pub(crate) content_hash: u64,
    pub(crate) keys: Vec<String>,
    pub(crate) card_ids: Vec<String>,
    pub(crate) recommended_key: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CrossDuplicateGroup {
    pub(crate) content_hash: u64,
    pub(crate) anchors: Vec<String>,
    pub(crate) keys: Vec<String>,
    pub(crate) card_ids: Vec<String>,
    pub(crate) recommended_anchor_id: String,
    pub(crate) recommended_key: String,
}

#[derive(Clone, Debug)]
pub(crate) struct OverloadedOutliersGroup {
    pub(crate) key: String,
    pub(crate) dominant_hash: u64,
    pub(crate) dominant_count: usize,
    pub(crate) total_count: usize,
    pub(crate) outlier_card_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct OverloadedKeySummary {
    pub(crate) key: String,
    pub(crate) anchor_count: usize,
    pub(crate) variant_count: usize,
}
