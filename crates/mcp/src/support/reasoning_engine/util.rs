#![forbid(unsafe_code)]

use crate::{LANE_TAG_AGENT_PREFIX, LANE_TAG_SHARED, VIS_TAG_DRAFT};
use serde_json::Value;

use super::types::{EngineAction, EngineRef, EngineSignal, priority_rank, severity_rank};

pub(super) fn card_tags_lower(card: &Value) -> Vec<String> {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        out.push(tag.to_ascii_lowercase());
    }
    out
}

pub(super) fn card_label(card: &Value) -> String {
    if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
        let title = title.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }
    card.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("CARD")
        .to_string()
}

pub(super) fn shorten(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = s.chars();
    let mut out = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

pub(super) fn ref_card(card_id: &str) -> EngineRef {
    EngineRef {
        kind: "card",
        id: card_id.to_string(),
    }
}

fn signal(
    code: &'static str,
    severity: &'static str,
    message: String,
    refs: Vec<EngineRef>,
) -> EngineSignal {
    EngineSignal {
        severity_rank: severity_rank(severity),
        sort_ts_ms: 0,
        code,
        severity,
        message,
        refs,
    }
}

pub(super) fn signal_at(
    code: &'static str,
    severity: &'static str,
    message: String,
    refs: Vec<EngineRef>,
    sort_ts_ms: i64,
) -> EngineSignal {
    let mut s = signal(code, severity, message, refs);
    s.sort_ts_ms = sort_ts_ms;
    s
}

fn action(
    kind: &'static str,
    priority: &'static str,
    title: String,
    why: Option<String>,
    refs: Vec<EngineRef>,
    calls: Vec<Value>,
) -> EngineAction {
    EngineAction {
        priority_rank: priority_rank(priority),
        sort_ts_ms: 0,
        kind,
        priority,
        title,
        why,
        refs,
        calls,
    }
}

pub(super) fn action_at(
    kind: &'static str,
    priority: &'static str,
    title: String,
    why: Option<String>,
    refs: Vec<EngineRef>,
    calls: Vec<Value>,
    sort_ts_ms: i64,
) -> EngineAction {
    let mut a = action(kind, priority, title, why, refs, calls);
    a.sort_ts_ms = sort_ts_ms;
    a
}

pub(super) fn ms_per_day() -> i64 {
    86_400_000
}

pub(super) fn max_ts_ms(cards: &[Value], trace_entries: &[Value]) -> i64 {
    let mut max_ts = 0i64;
    for card in cards {
        if let Some(ts) = card.get("last_ts_ms").and_then(|v| v.as_i64()) {
            max_ts = max_ts.max(ts);
        }
    }
    for entry in trace_entries {
        if let Some(ts) = entry.get("ts_ms").and_then(|v| v.as_i64()) {
            max_ts = max_ts.max(ts);
        }
    }
    max_ts
}

pub(super) fn extract_stale_after_ms_from_test_card(card: &Value) -> Option<i64> {
    fn ms_from_meta(meta: &Value) -> Option<i64> {
        if let Some(ms) = meta.get("stale_after_ms").and_then(|v| v.as_i64()) {
            return Some(ms.max(0));
        }
        if let Some(ms) = meta
            .get("run")
            .and_then(|v| v.get("stale_after_ms"))
            .and_then(|v| v.as_i64())
        {
            return Some(ms.max(0));
        }
        None
    }

    let meta = card.get("meta").unwrap_or(&Value::Null);
    if let Some(ms) = ms_from_meta(meta) {
        return Some(ms);
    }
    if let Some(inner) = meta.get("meta")
        && let Some(ms) = ms_from_meta(inner)
    {
        return Some(ms);
    }
    None
}

pub(super) fn extract_stale_after_days_from_test_card(card: &Value) -> Option<i64> {
    fn days_from_meta(meta: &Value) -> Option<i64> {
        if let Some(days) = meta.get("stale_after_days").and_then(|v| v.as_i64()) {
            return Some(days);
        }
        if let Some(days) = meta
            .get("run")
            .and_then(|v| v.get("stale_after_days"))
            .and_then(|v| v.as_i64())
        {
            return Some(days);
        }
        None
    }

    let meta = card.get("meta").unwrap_or(&Value::Null);
    if let Some(days) = days_from_meta(meta) {
        return Some(days);
    }
    if let Some(inner) = meta.get("meta")
        && let Some(days) = days_from_meta(inner)
    {
        return Some(days);
    }
    None
}

pub(super) fn refs_from_ids(ids: &[String], max: usize) -> Vec<EngineRef> {
    let mut out = Vec::new();
    for id in ids.iter().take(max) {
        out.push(ref_card(id));
    }
    out
}

pub(super) fn edge_triplet(edge: &Value) -> Option<(&str, &str, &str)> {
    let from = edge.get("from").and_then(|v| v.as_str())?;
    let rel = edge.get("rel").and_then(|v| v.as_str())?;
    let to = edge.get("to").and_then(|v| v.as_str())?;
    Some((from, rel, to))
}

pub(super) fn trace_has_progress_signal(trace_entries: &[Value]) -> bool {
    for entry in trace_entries.iter().rev().take(12) {
        if entry.get("kind").and_then(|v| v.as_str()) == Some("event")
            && entry.get("event_type").and_then(|v| v.as_str()) == Some("evidence_captured")
        {
            return true;
        }
        if entry.get("kind").and_then(|v| v.as_str()) == Some("note")
            && entry.get("format").and_then(|v| v.as_str()) == Some("think_card")
        {
            let ty = entry
                .get("meta")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str());
            if matches!(ty, Some("evidence" | "decision")) {
                return true;
            }
        }
    }
    false
}

pub(super) fn card_has_tag(card: &Value, tag: &str) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter().any(|t| t.as_str() == Some(tag))
}

pub(super) fn card_status_is_active_for_discipline(card: &Value) -> bool {
    let status = card
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open")
        .trim();
    !(status.eq_ignore_ascii_case("closed")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("resolved"))
}

pub(super) fn card_is_draft_like(card: &Value) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        // Missing tags => treat as canon/shared by default.
        return false;
    };

    let mut has_shared_lane = false;
    let mut has_legacy_agent_lane = false;
    let mut has_draft = false;

    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        if tag == VIS_TAG_DRAFT {
            has_draft = true;
        }
        if tag == LANE_TAG_SHARED {
            has_shared_lane = true;
        }
        if tag.starts_with(LANE_TAG_AGENT_PREFIX) {
            has_legacy_agent_lane = true;
        }
    }

    // If lane tags are malformed (multiple lanes), prefer shared to reduce false positives.
    if has_shared_lane {
        has_legacy_agent_lane = false;
    }

    has_draft || has_legacy_agent_lane
}
