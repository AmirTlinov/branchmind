#![forbid(unsafe_code)]

use std::collections::BTreeSet;

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
