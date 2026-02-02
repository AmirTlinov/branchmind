#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(crate) const VIS_TAG_DRAFT: &str = "v:draft";
pub(crate) const VIS_TAG_CANON: &str = "v:canon";
pub(crate) const ANCHOR_TAG_PREFIX: &str = "a:";
pub(crate) const ANCHOR_MAX_SLUG_LEN: usize = 64;
pub(crate) const KEY_TAG_PREFIX: &str = "k:";
pub(crate) const KEY_MAX_SLUG_LEN: usize = 64;

pub(crate) fn normalize_anchor_id_tag(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let lowered = raw.to_ascii_lowercase();
    let slug = lowered.strip_prefix(ANCHOR_TAG_PREFIX)?;
    if slug.is_empty() || slug.len() > ANCHOR_MAX_SLUG_LEN {
        return None;
    }

    let mut chars = slug.chars();
    let first = chars.next()?;
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return None;
    }
    for ch in chars {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            continue;
        }
        return None;
    }

    Some(format!("{ANCHOR_TAG_PREFIX}{slug}"))
}

pub(crate) fn normalize_key_id_tag(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let lowered = raw.to_ascii_lowercase();
    let slug = lowered.strip_prefix(KEY_TAG_PREFIX)?;
    if slug.is_empty() || slug.len() > KEY_MAX_SLUG_LEN {
        return None;
    }

    let mut chars = slug.chars();
    let first = chars.next()?;
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return None;
    }
    for ch in chars {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            continue;
        }
        return None;
    }

    Some(format!("{KEY_TAG_PREFIX}{slug}"))
}

pub(crate) fn tags_has(tags: &[String], needle: &str) -> bool {
    let needle = needle.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    tags.iter().any(|t| t.trim().to_ascii_lowercase() == needle)
}

pub(crate) fn tags_has_legacy_agent_lane(tags: &[String]) -> bool {
    tags.iter().any(|t| {
        t.trim()
            .to_ascii_lowercase()
            .starts_with(LANE_TAG_AGENT_PREFIX)
    })
}

pub(crate) fn tags_is_draft(tags: &[String]) -> bool {
    if tags_has(tags, VIS_TAG_DRAFT) {
        return true;
    }
    // Legacy lane semantics: `lane:agent:*` may exist on older artifacts.
    // It is treated as a draft marker unless explicitly promoted to canon.
    tags_has_legacy_agent_lane(tags) && !tags_has(tags, VIS_TAG_CANON)
}

pub(crate) fn tags_is_pinned(tags: &[String]) -> bool {
    tags_has(tags, PIN_TAG)
}

pub(crate) fn tags_visibility_allows(
    tags: &[String],
    include_drafts: bool,
    focus_step_tag: Option<&str>,
) -> bool {
    if include_drafts {
        return true;
    }
    if tags_is_pinned(tags) {
        return true;
    }
    if let Some(step_tag) = focus_step_tag.map(str::trim).filter(|t| !t.is_empty())
        && tags_has(tags, step_tag)
    {
        return true;
    }
    !tags_is_draft(tags)
}

fn meta_lane_kind(meta: &Value) -> Option<&str> {
    // Best-effort: lane stamps may exist at `meta.lane` (note tools) or `meta.meta.lane`
    // (think_card trace entries, where `meta` is a wrapper object).
    let direct = meta
        .get("lane")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("kind"))
        .and_then(|v| v.as_str());
    if direct.is_some() {
        return direct;
    }
    meta.get("meta")
        .and_then(|v| v.get("lane"))
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("kind"))
        .and_then(|v| v.as_str())
}

pub(crate) fn meta_is_draft(meta: &Value) -> bool {
    meta_lane_kind(meta) == Some("agent")
}

pub(crate) fn card_value_visibility_allows(
    card: &Value,
    include_drafts: bool,
    focus_step_tag: Option<&str>,
) -> bool {
    if include_drafts {
        return true;
    }
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return true;
    };

    let focus_step_tag = focus_step_tag.map(str::trim).filter(|t| !t.is_empty());

    let mut pinned = false;
    let mut step_scoped = false;
    let mut canon = false;
    let mut explicit_draft = false;
    let mut legacy_lane = false;

    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        if tag == PIN_TAG {
            pinned = true;
            break;
        }
        if Some(tag.as_str()) == focus_step_tag {
            step_scoped = true;
        }
        if tag == VIS_TAG_CANON {
            canon = true;
        }
        if tag == VIS_TAG_DRAFT {
            explicit_draft = true;
        }
        if tag.starts_with(LANE_TAG_AGENT_PREFIX) {
            legacy_lane = true;
        }
    }

    let draft = explicit_draft || (legacy_lane && !canon);
    pinned || step_scoped || !draft
}

pub(crate) fn card_value_anchor_tags(card: &Value) -> Vec<String> {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut out = std::collections::BTreeSet::<String>::new();
    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        if let Some(id) = normalize_anchor_id_tag(tag) {
            out.insert(id);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_lane_is_draft_unless_canon() {
        let lane = "lane:agent:alice".to_string();
        let canon = VIS_TAG_CANON.to_string();

        assert!(
            tags_is_draft(std::slice::from_ref(&lane)),
            "legacy lane should be treated as draft"
        );
        assert!(
            !tags_is_draft(&[lane.clone(), canon.clone()]),
            "legacy lane should not be draft when v:canon is present"
        );
        assert!(
            tags_visibility_allows(&[lane.clone(), canon.clone()], false, None),
            "v:canon should allow legacy-lane artifacts in default (non-audit) views"
        );
    }

    #[test]
    fn card_value_visibility_allows_canon_over_legacy_lane_but_not_over_explicit_draft() {
        let card = json!({
            "id": "CARD-1",
            "tags": ["lane:agent:alice", "v:canon"]
        });
        assert!(card_value_visibility_allows(&card, false, None));

        let card = json!({
            "id": "CARD-2",
            "tags": ["lane:agent:alice", "v:canon", "v:draft"]
        });
        assert!(
            !card_value_visibility_allows(&card, false, None),
            "explicit v:draft should remain hidden unless include_drafts=true"
        );
        assert!(card_value_visibility_allows(&card, true, None));
    }

    #[test]
    fn normalize_anchor_id_tag_filters_invalid_ids() {
        assert_eq!(
            normalize_anchor_id_tag("a:core"),
            Some("a:core".to_string())
        );
        assert_eq!(
            normalize_anchor_id_tag("A:Core"),
            Some("a:core".to_string())
        );
        assert_eq!(
            normalize_anchor_id_tag("a:storage-sqlite"),
            Some("a:storage-sqlite".to_string())
        );

        // Invalid: empty slug or invalid chars / hierarchy.
        assert_eq!(normalize_anchor_id_tag("a:"), None);
        assert_eq!(normalize_anchor_id_tag("a:-bad"), None);
        assert_eq!(normalize_anchor_id_tag("a:storage/sqlite"), None);
        assert_eq!(normalize_anchor_id_tag("a:with space"), None);
    }
}
