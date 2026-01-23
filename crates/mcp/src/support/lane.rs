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
