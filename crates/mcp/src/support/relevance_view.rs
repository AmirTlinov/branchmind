#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelevanceView {
    Smart,
    Explore,
    Audit,
}

impl RelevanceView {
    pub(crate) fn warm_archive(self) -> bool {
        matches!(self, Self::Explore)
    }

    pub(crate) fn implies_all_lanes(self) -> bool {
        matches!(self, Self::Audit)
    }
}

pub(crate) fn parse_relevance_view(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
    default: RelevanceView,
) -> Result<RelevanceView, Value> {
    let raw = optional_string(args_obj, key)?;
    let Some(raw) = raw else {
        return Ok(default);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default);
    }
    match raw.to_ascii_lowercase().as_str() {
        "smart" => Ok(RelevanceView::Smart),
        "explore" => Ok(RelevanceView::Explore),
        "audit" => Ok(RelevanceView::Audit),
        _ => Err(ai_error_with(
            "INVALID_INPUT",
            "Unsupported view",
            Some("Supported: smart, explore, audit"),
            Vec::new(),
        )),
    }
}
