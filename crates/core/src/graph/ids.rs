#![forbid(unsafe_code)]

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
