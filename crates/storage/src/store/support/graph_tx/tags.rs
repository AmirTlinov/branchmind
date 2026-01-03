#![forbid(unsafe_code)]

use super::super::super::StoreError;
use bm_core::graph::{GraphTagError, normalize_tags as core_normalize_tags};

pub(in crate::store) fn normalize_tags(tags: &[String]) -> Result<Vec<String>, StoreError> {
    core_normalize_tags(tags).map_err(|err| match err {
        GraphTagError::ContainsPipe => StoreError::InvalidInput(err.message()),
        GraphTagError::ContainsControl => StoreError::InvalidInput(err.message()),
    })
}

pub(in crate::store) fn encode_tags(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    Some(format!("\n{}\n", tags.join("\n")))
}

pub(in crate::store) fn decode_tags(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split('\n')
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect()
}
