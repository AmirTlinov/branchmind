#![forbid(unsafe_code)]

use crate::{LANE_TAG_AGENT_PREFIX, LANE_TAG_SHARED, PIN_TAG, lane_meta_value};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct CardMini {
    id: String,
    card_type: String,
    title: String,
    last_ts_ms: i64,
}

impl CardMini {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "type": self.card_type,
            "title": self.title
        })
    }
}

#[derive(Clone, Debug)]
struct LaneAcc {
    lane: Value,
    cards: usize,
    open: usize,
    pinned: usize,
    decisions: usize,
    evidence: usize,
    blockers: usize,
    last_ts_ms: i64,
    top_pinned: Option<CardMini>,
    top_open: Option<CardMini>,
}

fn lane_key_and_meta(card: &Value) -> (String, Value) {
    let mut lane_tags = Vec::<String>::new();
    if let Some(tags) = card.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            let Some(tag) = tag.as_str() else {
                continue;
            };
            let tag = tag.trim().to_ascii_lowercase();
            if tag.starts_with("lane:") {
                lane_tags.push(tag);
            }
        }
    }

    if lane_tags.is_empty() || lane_tags.iter().any(|t| t == LANE_TAG_SHARED) {
        return ("shared".to_string(), lane_meta_value(None));
    }

    if let Some(agent_tag) = lane_tags
        .iter()
        .find(|t| t.starts_with(LANE_TAG_AGENT_PREFIX))
    {
        let agent_id = agent_tag
            .get(LANE_TAG_AGENT_PREFIX.len()..)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !agent_id.is_empty() {
            return (
                format!("agent:{agent_id}"),
                lane_meta_value(Some(agent_id.as_str())),
            );
        }
    }

    let unknown = lane_tags
        .first()
        .cloned()
        .unwrap_or_else(|| "lane:unknown".to_string());
    (
        format!("unknown:{unknown}"),
        json!({ "kind": "unknown", "tag": unknown }),
    )
}

fn card_is_pinned(card: &Value) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter()
        .filter_map(|t| t.as_str())
        .any(|t| t.trim().eq_ignore_ascii_case(PIN_TAG))
}

fn card_is_blocker(card: &Value) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter()
        .filter_map(|t| t.as_str())
        .any(|t| t.trim().eq_ignore_ascii_case("blocker"))
}

fn card_status_is_open(card: &Value) -> bool {
    card.get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open")
        .eq_ignore_ascii_case("open")
}

fn card_mini(card: &Value) -> Option<CardMini> {
    let id = card.get("id").and_then(|v| v.as_str())?.trim();
    if id.is_empty() {
        return None;
    }
    let card_type = card
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("card")
        .trim()
        .to_string();
    let title = card
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let last_ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);

    Some(CardMini {
        id: id.to_string(),
        card_type,
        title,
        last_ts_ms,
    })
}

fn update_top_slot(slot: &mut Option<CardMini>, candidate: CardMini) {
    let replace = match slot {
        None => true,
        Some(existing) => candidate
            .last_ts_ms
            .cmp(&existing.last_ts_ms)
            .then_with(|| existing.id.cmp(&candidate.id))
            .is_gt(),
    };
    if replace {
        *slot = Some(candidate);
    }
}

pub(crate) fn build_lane_summary(cards: &[Value], lanes_limit: usize) -> Value {
    if lanes_limit == 0 {
        return json!({
            "mode": "slice",
            "lanes_total": 0,
            "lanes": [],
            "truncated": false
        });
    }

    let mut seen_ids = std::collections::BTreeSet::<String>::new();
    let mut by_lane = std::collections::BTreeMap::<String, LaneAcc>::new();

    for card in cards {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if !seen_ids.insert(id.to_string()) {
            continue;
        }

        let (lane_key, lane_meta) = lane_key_and_meta(card);
        let acc = by_lane.entry(lane_key).or_insert_with(|| LaneAcc {
            lane: lane_meta,
            cards: 0,
            open: 0,
            pinned: 0,
            decisions: 0,
            evidence: 0,
            blockers: 0,
            last_ts_ms: 0,
            top_pinned: None,
            top_open: None,
        });

        acc.cards += 1;
        let pinned = card_is_pinned(card);
        let open = card_status_is_open(card);
        if open {
            acc.open += 1;
        }
        if pinned {
            acc.pinned += 1;
        }

        match card.get("type").and_then(|v| v.as_str()) {
            Some("decision") => acc.decisions += 1,
            Some("evidence") => acc.evidence += 1,
            _ => {}
        }
        if card_is_blocker(card) {
            acc.blockers += 1;
        }

        let ts = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        acc.last_ts_ms = acc.last_ts_ms.max(ts);

        let Some(mini) = card_mini(card) else {
            continue;
        };

        if pinned {
            update_top_slot(&mut acc.top_pinned, mini.clone());
        }
        if open && !pinned {
            update_top_slot(&mut acc.top_open, mini);
        } else if open && acc.top_open.is_none() {
            // Fallback: if there are no open non-pinned cards, allow pinned-open as a last resort.
            update_top_slot(&mut acc.top_open, mini);
        }
    }

    let lanes_total = by_lane.len();
    let mut shared: Option<LaneAcc> = None;
    let mut others = Vec::<(String, LaneAcc)>::new();
    for (key, acc) in by_lane {
        if key == "shared" {
            shared = Some(acc);
        } else {
            others.push((key, acc));
        }
    }
    others.sort_by(|(ka, a), (kb, b)| b.last_ts_ms.cmp(&a.last_ts_ms).then_with(|| ka.cmp(kb)));

    let mut lanes = Vec::<Value>::new();
    if let Some(acc) = shared {
        lanes.push(json!({
            "lane": acc.lane,
            "counts": {
                "cards": acc.cards,
                "open": acc.open,
                "pinned": acc.pinned,
                "decisions": acc.decisions,
                "evidence": acc.evidence,
                "blockers": acc.blockers
            },
            "top": {
                "pinned": acc.top_pinned.as_ref().map(|c| c.to_json()).unwrap_or(Value::Null),
                "open": acc.top_open.as_ref().map(|c| c.to_json()).unwrap_or(Value::Null)
            },
            "last_ts_ms": acc.last_ts_ms
        }));
    }
    for (_key, acc) in others {
        lanes.push(json!({
            "lane": acc.lane,
            "counts": {
                "cards": acc.cards,
                "open": acc.open,
                "pinned": acc.pinned,
                "decisions": acc.decisions,
                "evidence": acc.evidence,
                "blockers": acc.blockers
            },
            "top": {
                "pinned": acc.top_pinned.as_ref().map(|c| c.to_json()).unwrap_or(Value::Null),
                "open": acc.top_open.as_ref().map(|c| c.to_json()).unwrap_or(Value::Null)
            },
            "last_ts_ms": acc.last_ts_ms
        }));
    }

    let mut truncated = false;
    if lanes.len() > lanes_limit {
        truncated = true;
        lanes.truncate(lanes_limit);
    }

    json!({
        "mode": "slice",
        "lanes_total": lanes_total,
        "lanes": lanes,
        "truncated": truncated
    })
}
