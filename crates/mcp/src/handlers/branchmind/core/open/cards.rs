#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) fn card_type(card: &Value) -> &str {
    card.get("type").and_then(|v| v.as_str()).unwrap_or("note")
}

pub(super) fn card_ts(card: &Value) -> i64 {
    card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0)
}

pub(super) fn card_id(card: &Value) -> &str {
    card.get("id").and_then(|v| v.as_str()).unwrap_or("")
}

pub(super) fn card_has_tag(card: &Value, tag: &str) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter().any(|t| {
        t.as_str()
            .map(|s| s.eq_ignore_ascii_case(tag))
            .unwrap_or(false)
    })
}

pub(super) fn is_canon_by_type(card: &Value) -> bool {
    matches!(card_type(card), "decision" | "evidence" | "test")
}

pub(super) fn is_canon_by_visibility(card: &Value) -> bool {
    card_has_tag(card, VIS_TAG_CANON)
}

pub(super) fn is_draft_by_visibility(card: &Value) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };

    let mut has_canon = false;
    let mut explicit_draft = false;
    let mut legacy_lane = false;

    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        if tag == VIS_TAG_CANON {
            has_canon = true;
        }
        if tag == VIS_TAG_DRAFT {
            explicit_draft = true;
        }
        if tag.starts_with(LANE_TAG_AGENT_PREFIX) {
            legacy_lane = true;
        }
    }

    explicit_draft || (legacy_lane && !has_canon)
}
