#![forbid(unsafe_code)]

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GraphNodeId(String);

impl GraphNodeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, GraphNodeIdError> {
        let value = value.into();
        validate_node_id(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphNodeIdError {
    Empty,
    TooLong,
    ContainsPipe,
    ContainsControl,
}

impl GraphNodeIdError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "node id must not be empty",
            Self::TooLong => "node id is too long",
            Self::ContainsPipe => "node id must not contain '|'",
            Self::ContainsControl => "node id contains control characters",
        }
    }
}

fn validate_node_id(value: &str) -> Result<(), GraphNodeIdError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GraphNodeIdError::Empty);
    }
    if trimmed.len() > 256 {
        return Err(GraphNodeIdError::TooLong);
    }
    if trimmed.contains('|') {
        return Err(GraphNodeIdError::ContainsPipe);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(GraphNodeIdError::ContainsControl);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GraphType(String);

impl GraphType {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, GraphTypeError> {
        let value = value.into();
        validate_graph_type(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphTypeError {
    Empty,
    TooLong,
    ContainsControl,
}

impl GraphTypeError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "node type must not be empty",
            Self::TooLong => "node type is too long",
            Self::ContainsControl => "node type contains control characters",
        }
    }
}

fn validate_graph_type(value: &str) -> Result<(), GraphTypeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GraphTypeError::Empty);
    }
    if trimmed.len() > 128 {
        return Err(GraphTypeError::TooLong);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(GraphTypeError::ContainsControl);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GraphRel(String);

impl GraphRel {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, GraphRelError> {
        let value = value.into();
        validate_graph_rel(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphRelError {
    Empty,
    TooLong,
    ContainsPipe,
    ContainsControl,
}

impl GraphRelError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "edge rel must not be empty",
            Self::TooLong => "edge rel is too long",
            Self::ContainsPipe => "edge rel must not contain '|'",
            Self::ContainsControl => "edge rel contains control characters",
        }
    }
}

fn validate_graph_rel(value: &str) -> Result<(), GraphRelError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GraphRelError::Empty);
    }
    if trimmed.len() > 128 {
        return Err(GraphRelError::TooLong);
    }
    if trimmed.contains('|') {
        return Err(GraphRelError::ContainsPipe);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(GraphRelError::ContainsControl);
    }
    Ok(())
}

pub fn normalize_tags(tags: &[String]) -> Result<Vec<String>, GraphTagError> {
    let mut out = BTreeSet::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err(GraphTagError::ContainsControl);
        }
        if trimmed.contains('|') {
            return Err(GraphTagError::ContainsPipe);
        }
        out.insert(trimmed.to_lowercase());
    }
    Ok(out.into_iter().collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphTagError {
    ContainsPipe,
    ContainsControl,
}

impl GraphTagError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::ContainsPipe => "tag must not contain '|'",
            Self::ContainsControl => "tag contains control characters",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConflictId(String);

impl ConflictId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, ConflictIdError> {
        let value = value.into();
        validate_conflict_id(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConflictIdError {
    Empty,
    InvalidFormat,
}

impl ConflictIdError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "conflict_id must not be empty",
            Self::InvalidFormat => "conflict_id must match CONFLICT-[0-9a-f]{32}",
        }
    }
}

fn validate_conflict_id(value: &str) -> Result<(), ConflictIdError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConflictIdError::Empty);
    }
    let Some(hex) = trimmed.strip_prefix("CONFLICT-") else {
        return Err(ConflictIdError::InvalidFormat);
    };
    if hex.len() != 32 {
        return Err(ConflictIdError::InvalidFormat);
    }
    if !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(ConflictIdError::InvalidFormat);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct GraphNode {
    pub id: String,
    pub node_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub meta_json: Option<String>,
    pub deleted: bool,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub from: String,
    pub rel: String,
    pub to: String,
    pub meta_json: Option<String>,
    pub deleted: bool,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub enum GraphOp {
    NodeUpsert(GraphNodeUpsert),
    NodeDelete {
        id: String,
    },
    EdgeUpsert(GraphEdgeUpsert),
    EdgeDelete {
        from: String,
        rel: String,
        to: String,
    },
}

#[derive(Clone, Debug)]
pub struct GraphNodeUpsert {
    pub id: String,
    pub node_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GraphEdgeUpsert {
    pub from: String,
    pub rel: String,
    pub to: String,
    pub meta_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GraphApplyResult {
    pub nodes_upserted: usize,
    pub nodes_deleted: usize,
    pub edges_upserted: usize,
    pub edges_deleted: usize,
    pub last_seq: i64,
    pub last_ts_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphQuerySlice {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphQueryRequest {
    pub ids: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub status: Option<String>,
    pub tags_any: Option<Vec<String>>,
    pub tags_all: Option<Vec<String>>,
    pub text: Option<String>,
    pub cursor: Option<i64>,
    pub limit: usize,
    pub include_edges: bool,
    pub edges_limit: usize,
}

#[derive(Clone, Debug)]
pub struct GraphValidateError {
    pub code: &'static str,
    pub message: String,
    pub kind: &'static str,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct GraphValidateResult {
    pub ok: bool,
    pub nodes: usize,
    pub edges: usize,
    pub errors: Vec<GraphValidateError>,
}

#[derive(Clone, Debug)]
pub enum GraphDiffChange {
    Node { to: GraphNode },
    Edge { to: GraphEdge },
}

#[derive(Clone, Debug)]
pub struct GraphDiffSlice {
    pub changes: Vec<GraphDiffChange>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphMergeResult {
    pub merged: usize,
    pub skipped: usize,
    pub conflicts_created: usize,
    pub conflict_ids: Vec<String>,
    pub count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct GraphConflictSummary {
    pub conflict_id: String,
    pub kind: String,
    pub key: String,
    pub status: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct GraphConflictDetail {
    pub conflict_id: String,
    pub kind: String,
    pub key: String,
    pub from_branch: String,
    pub into_branch: String,
    pub doc: String,
    pub status: String,
    pub created_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
    pub base_node: Option<GraphNode>,
    pub theirs_node: Option<GraphNode>,
    pub ours_node: Option<GraphNode>,
    pub base_edge: Option<GraphEdge>,
    pub theirs_edge: Option<GraphEdge>,
    pub ours_edge: Option<GraphEdge>,
}

#[derive(Clone, Debug)]
pub struct GraphConflictResolveResult {
    pub conflict_id: String,
    pub status: String,
    pub applied: bool,
    pub applied_seq: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_node_id_validation() {
        assert_eq!(
            GraphNodeId::try_new("").unwrap_err(),
            GraphNodeIdError::Empty
        );
        assert_eq!(
            GraphNodeId::try_new("  ").unwrap_err(),
            GraphNodeIdError::Empty
        );
        assert_eq!(
            GraphNodeId::try_new("bad|id").unwrap_err(),
            GraphNodeIdError::ContainsPipe
        );
        assert_eq!(
            GraphNodeId::try_new("bad\u{0007}id").unwrap_err(),
            GraphNodeIdError::ContainsControl
        );
        assert!(GraphNodeId::try_new("CARD-123").is_ok());
    }

    #[test]
    fn graph_rel_validation() {
        assert_eq!(GraphRel::try_new("").unwrap_err(), GraphRelError::Empty);
        assert_eq!(
            GraphRel::try_new("bad|rel").unwrap_err(),
            GraphRelError::ContainsPipe
        );
        assert_eq!(
            GraphRel::try_new("bad\u{0000}rel").unwrap_err(),
            GraphRelError::ContainsControl
        );
        assert!(GraphRel::try_new("supports").is_ok());
    }

    #[test]
    fn conflict_id_validation() {
        assert_eq!(ConflictId::try_new("").unwrap_err(), ConflictIdError::Empty);
        assert_eq!(
            ConflictId::try_new("CONFLICT-xyz").unwrap_err(),
            ConflictIdError::InvalidFormat
        );
        assert!(ConflictId::try_new("CONFLICT-0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn normalize_tags_is_deterministic_and_safe() {
        let out = normalize_tags(&[
            " Foo ".to_string(),
            "foo".to_string(),
            "BAR".to_string(),
            "".to_string(),
        ])
        .unwrap();
        assert_eq!(out, vec!["bar".to_string(), "foo".to_string()]);

        assert_eq!(
            normalize_tags(&["bad|tag".to_string()]).unwrap_err(),
            GraphTagError::ContainsPipe
        );
    }
}
