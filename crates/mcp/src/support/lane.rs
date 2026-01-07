#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) const LANE_TAG_SHARED: &str = "lane:shared";
pub(crate) const LANE_TAG_PREFIX: &str = "lane:";
pub(crate) const LANE_TAG_AGENT_PREFIX: &str = "lane:agent:";

pub(crate) fn lane_tag_for_agent(agent_id: &str) -> String {
    format!("{LANE_TAG_AGENT_PREFIX}{agent_id}")
}

pub(crate) fn lane_meta_value(agent_id: Option<&str>) -> Value {
    match agent_id {
        Some(agent_id) => json!({ "kind": "agent", "agent_id": agent_id }),
        None => json!({ "kind": "shared" }),
    }
}

pub(crate) fn is_lane_tag(tag: &str) -> bool {
    tag.trim().to_ascii_lowercase().starts_with(LANE_TAG_PREFIX)
}

pub(crate) fn lane_matches_tags(tags: &[String], agent_id: Option<&str>) -> bool {
    let mut has_lane = false;
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase();
        if !tag.starts_with(LANE_TAG_PREFIX) {
            continue;
        }
        has_lane = true;
        if tag == LANE_TAG_SHARED {
            return true;
        }
        if let Some(agent_id) = agent_id
            && tag == lane_tag_for_agent(agent_id)
        {
            return true;
        }
    }
    // Legacy behavior: cards without a lane tag are treated as shared.
    !has_lane
}

pub(crate) fn apply_lane_stamp_to_tags(tags: &mut Vec<String>, agent_id: Option<&str>) {
    use std::collections::BTreeSet;

    let target_lane = match agent_id {
        Some(agent_id) => lane_tag_for_agent(agent_id),
        None => LANE_TAG_SHARED.to_string(),
    };

    let mut out = BTreeSet::<String>::new();
    for tag in tags.drain(..) {
        if is_lane_tag(&tag) {
            continue;
        }
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.insert(trimmed.to_ascii_lowercase());
    }
    out.insert(target_lane);
    *tags = out.into_iter().collect();
}

pub(crate) fn apply_lane_stamp_to_meta(meta_value: &mut Value, agent_id: Option<&str>) {
    let Value::Object(obj) = meta_value else {
        return;
    };
    obj.insert("lane".to_string(), lane_meta_value(agent_id));
}

pub(crate) fn lane_matches_card_value(card: &Value, agent_id: Option<&str>) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return true;
    };

    let mut has_lane = false;
    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        if !tag.starts_with(LANE_TAG_PREFIX) {
            continue;
        }
        has_lane = true;
        if tag == LANE_TAG_SHARED {
            return true;
        }
        if let Some(agent_id) = agent_id
            && tag == lane_tag_for_agent(agent_id)
        {
            return true;
        }
    }

    !has_lane
}

pub(crate) fn lane_matches_meta(meta: &Value, agent_id: Option<&str>) -> bool {
    // Lane filtering for note-like entries that carry a `meta.lane` stamp.
    //
    // Legacy behavior: missing `meta.lane` is treated as shared.
    let Some(lane) = meta.get("lane") else {
        return true;
    };
    let Some(obj) = lane.as_object() else {
        return true;
    };

    match obj.get("kind").and_then(|v| v.as_str()) {
        Some("shared") => true,
        Some("agent") => match (agent_id, obj.get("agent_id").and_then(|v| v.as_str())) {
            (Some(agent_id), Some(stamped)) => agent_id == stamped,
            _ => false,
        },
        _ => true,
    }
}
