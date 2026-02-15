#![forbid(unsafe_code)]

use crate::card_value_anchor_tags;
use crate::support::ai::suggest_call;
use crate::support::proof::looks_like_bare_url;
use crate::{LANE_TAG_AGENT_PREFIX, LANE_TAG_SHARED, PIN_TAG, VIS_TAG_DRAFT};
use serde_json::{Value, json};

pub(crate) const REASONING_ENGINE_VERSION: &str = "v0.5";
mod derive;
pub(crate) use derive::derive_reasoning_engine;

mod step_aware;
pub(crate) use step_aware::{derive_reasoning_engine_step_aware, filter_engine_to_cards};

#[derive(Clone, Copy, Debug)]
pub(crate) struct EngineLimits {
    pub(crate) signals_limit: usize,
    pub(crate) actions_limit: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EngineScope<'a> {
    pub(crate) workspace: &'a str,
    pub(crate) branch: &'a str,
    pub(crate) graph_doc: &'a str,
    pub(crate) trace_doc: &'a str,
}

#[derive(Clone, Debug)]
struct EngineSignal {
    severity_rank: u8,
    sort_ts_ms: i64,
    code: &'static str,
    severity: &'static str,
    message: String,
    refs: Vec<EngineRef>,
}

#[derive(Clone, Debug)]
struct EngineAction {
    priority_rank: u8,
    sort_ts_ms: i64,
    kind: &'static str,
    priority: &'static str,
    title: String,
    why: Option<String>,
    refs: Vec<EngineRef>,
    calls: Vec<Value>,
}

#[derive(Clone, Debug)]
struct EngineRef {
    kind: &'static str,
    id: String,
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 4,
        "high" => 3,
        "warning" => 2,
        "info" => 1,
        _ => 0,
    }
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn strip_markdown_list_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return rest.trim_start();
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return rest.trim_start();
    }
    let bytes = trimmed.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && idx + 1 < bytes.len() && bytes[idx] == b'.' && bytes[idx + 1] == b' ' {
        return trimmed[idx + 2..].trim_start();
    }
    trimmed
}

fn looks_like_placeholder(value: &str) -> bool {
    let v = value.trim();
    v.is_empty() || v.contains("<fill")
}

fn strip_markdownish_prefixes(line: &str) -> &str {
    let mut s = line.trim_start();

    if let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }

    for prefix in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest.trim_start();
        }
    }

    let bytes = s.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0
        && idx + 1 < bytes.len()
        && (bytes[idx] == b'.' || bytes[idx] == b')')
        && bytes[idx + 1] == b' '
    {
        return s[(idx + 2)..].trim_start();
    }

    s
}

#[derive(Clone, Copy, Debug, Default)]
struct EvidenceReceipts {
    cmd: bool,
    link: bool,
    cmd_placeholder: bool,
    link_placeholder: bool,
}

fn receipts_from_text(text: &str) -> EvidenceReceipts {
    let mut out = EvidenceReceipts::default();
    for raw in text.lines() {
        let trimmed = strip_markdownish_prefixes(raw).trim();
        if trimmed.is_empty() {
            continue;
        }

        let bytes = trimmed.as_bytes();
        if bytes
            .get(0..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"CMD:"))
        {
            let rest = trimmed.get(4..).unwrap_or_default().trim();
            if looks_like_placeholder(rest) {
                out.cmd_placeholder = true;
            } else if !rest.is_empty() {
                out.cmd = true;
            }
            continue;
        }
        if bytes
            .get(0..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"LINK:"))
        {
            let rest = trimmed.get(5..).unwrap_or_default().trim();
            if looks_like_placeholder(rest) {
                out.link_placeholder = true;
            } else if !rest.is_empty() {
                out.link = true;
            }
            continue;
        }

        // Bare URLs are accepted as LINK receipts for scoring (not as CMD receipts).
        if looks_like_bare_url(trimmed) {
            out.link = true;
        }
    }
    out
}

fn card_tags_lower(card: &Value) -> Vec<String> {
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

fn meta_str<'a>(root: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let mut cur = root;
    for key in keys {
        cur = cur.get(*key)?;
    }
    cur.as_str()
}

fn evidence_source_hint(card: &Value) -> Option<&'static str> {
    let tags = card_tags_lower(card);
    if tags
        .iter()
        .any(|t| t == "ci" || t == "github-actions" || t == "buildkite" || t == "jenkins")
    {
        return Some("ci");
    }
    if tags.iter().any(|t| t == "local") {
        return Some("local");
    }

    let meta = card.get("meta").unwrap_or(&Value::Null);
    if let Some(env) = meta_str(meta, &["run", "env"]).or_else(|| meta_str(meta, &["env"])) {
        let env = env.trim().to_ascii_lowercase();
        if env == "ci" {
            return Some("ci");
        }
        if env == "local" {
            return Some("local");
        }
    }

    None
}

fn evidence_receipts(card: &Value) -> EvidenceReceipts {
    let mut out = EvidenceReceipts::default();

    let meta = card.get("meta").unwrap_or(&Value::Null);
    for cmd in [
        meta_str(meta, &["run", "cmd"]),
        meta_str(meta, &["cmd"]),
        meta_str(meta, &["meta", "run", "cmd"]),
        meta_str(meta, &["meta", "cmd"]),
    ]
    .into_iter()
    .flatten()
    {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            continue;
        }
        if looks_like_placeholder(cmd) {
            out.cmd_placeholder = true;
        } else {
            out.cmd = true;
        }
    }

    for link in [
        meta_str(meta, &["run", "link"]),
        meta_str(meta, &["run", "url"]),
        meta_str(meta, &["link"]),
        meta_str(meta, &["url"]),
        meta_str(meta, &["artifact"]),
        meta_str(meta, &["meta", "run", "link"]),
        meta_str(meta, &["meta", "run", "url"]),
        meta_str(meta, &["meta", "link"]),
        meta_str(meta, &["meta", "url"]),
    ]
    .into_iter()
    .flatten()
    {
        let link = link.trim();
        if link.is_empty() {
            continue;
        }
        if looks_like_placeholder(link) {
            out.link_placeholder = true;
        } else if looks_like_bare_url(link) {
            out.link = true;
        } else {
            // Accept non-URL non-placeholder as "some link-like evidence" (e.g. artifact id),
            // but score it lower than a URL.
            out.link = true;
        }
    }

    if let Some(text) = card.get("text").and_then(|v| v.as_str()) {
        let from_text = receipts_from_text(text);
        out.cmd |= from_text.cmd;
        out.link |= from_text.link;
        out.cmd_placeholder |= from_text.cmd_placeholder;
        out.link_placeholder |= from_text.link_placeholder;
    }

    out
}

fn evidence_strength_score(
    evidence_card: &Value,
    outgoing_supports: &std::collections::BTreeMap<String, Vec<String>>,
    outgoing_blocks: &std::collections::BTreeMap<String, Vec<String>>,
    by_id: &std::collections::BTreeMap<String, &Value>,
) -> u8 {
    let receipts = evidence_receipts(evidence_card);
    let mut score = 0i32;

    if receipts.cmd {
        score += 25;
    }
    if receipts.link {
        score += 25;
    }

    match evidence_source_hint(evidence_card) {
        Some("ci") => score += 20,
        Some("local") => score += 10,
        _ => {}
    }

    // Evidence that supports runnable/test nodes is higher-value (more reproducible).
    if let Some(id) = evidence_card.get("id").and_then(|v| v.as_str()) {
        let mut targets = Vec::<String>::new();
        if let Some(out) = outgoing_supports.get(id) {
            targets.extend(out.iter().cloned());
        }
        if let Some(out) = outgoing_blocks.get(id) {
            targets.extend(out.iter().cloned());
        }
        if !targets.is_empty() {
            let mut supports_test = false;
            let mut supports_claim = false;
            for to in targets {
                let Some(card) = by_id.get(to.as_str()).copied() else {
                    continue;
                };
                match card.get("type").and_then(|v| v.as_str()) {
                    Some("test") => supports_test = true,
                    Some("hypothesis" | "decision") => supports_claim = true,
                    _ => {}
                }
            }
            if supports_test {
                score += 20;
            } else if supports_claim {
                score += 10;
            }
        }
    }

    score = score.clamp(0, 100);
    score as u8
}

fn weight_for_type(card_type: &str) -> f64 {
    match card_type {
        "evidence" => 4.0,
        "test" => 1.5,
        "decision" | "hypothesis" => 0.75,
        "question" | "subgoal" | "frame" | "update" | "note" => 0.25,
        _ => 0.25,
    }
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn confidence_key(node_id: &str, depth: usize) -> String {
    format!("{node_id}|{depth}")
}

fn card_type_of(card: &Value) -> &str {
    card.get("type").and_then(|v| v.as_str()).unwrap_or("card")
}

struct ConfidenceContext<'a, 'v> {
    by_id: &'a std::collections::BTreeMap<String, &'v Value>,
    incoming_supports: &'a std::collections::BTreeMap<String, Vec<String>>,
    incoming_blocks: &'a std::collections::BTreeMap<String, Vec<String>>,
    evidence_scores: &'a std::collections::BTreeMap<String, u8>,
}

fn confidence_for_id(
    node_id: &str,
    depth: usize,
    ctx: &ConfidenceContext<'_, '_>,
    memo: &mut std::collections::BTreeMap<String, f64>,
    stack: &mut std::collections::BTreeSet<String>,
) -> f64 {
    if node_id.trim().is_empty() {
        return 0.5;
    }
    let key = confidence_key(node_id, depth);
    if let Some(v) = memo.get(&key) {
        return *v;
    }
    if stack.contains(node_id) {
        return 0.5;
    }
    stack.insert(node_id.to_string());

    let card = ctx.by_id.get(node_id).copied();
    let card_type = card.map(card_type_of).unwrap_or("card");
    let base = if card_type == "evidence" {
        ctx.evidence_scores
            .get(node_id)
            .copied()
            .map(|v| v as f64 / 100.0)
            .unwrap_or(0.5)
    } else {
        0.5
    };

    let out = if card_type == "evidence" || depth == 0 {
        base
    } else {
        let mut pos = 1.0;
        let mut neg = 1.0;

        if let Some(ids) = ctx.incoming_supports.get(node_id) {
            for from in ids {
                let from_card = ctx.by_id.get(from).copied();
                let from_type = from_card.map(card_type_of).unwrap_or("card");
                let w = weight_for_type(from_type);
                let c = confidence_for_id(from, depth.saturating_sub(1), ctx, memo, stack);
                pos += w * c;
            }
        }
        if let Some(ids) = ctx.incoming_blocks.get(node_id) {
            for from in ids {
                let from_card = ctx.by_id.get(from).copied();
                let from_type = from_card.map(card_type_of).unwrap_or("card");
                let w = weight_for_type(from_type);
                let c = confidence_for_id(from, depth.saturating_sub(1), ctx, memo, stack);
                neg += w * c;
            }
        }

        pos / (pos + neg)
    };

    stack.remove(node_id);
    let out = clamp01(out);
    memo.insert(key, out);
    out
}

fn extract_cmd_from_test_card(card: &Value) -> Option<String> {
    fn cmd_from_meta(meta: &Value) -> Option<String> {
        if let Some(cmd) = meta
            .get("run")
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str())
        {
            let cmd = cmd.trim();
            if !looks_like_placeholder(cmd) {
                return Some(cmd.to_string());
            }
        }
        if let Some(cmd) = meta.get("cmd").and_then(|v| v.as_str()) {
            let cmd = cmd.trim();
            if !looks_like_placeholder(cmd) {
                return Some(cmd.to_string());
            }
        }
        None
    }

    let meta = card.get("meta").unwrap_or(&Value::Null);
    if let Some(cmd) = cmd_from_meta(meta) {
        return Some(cmd);
    }
    if let Some(inner) = meta.get("meta")
        && let Some(cmd) = cmd_from_meta(inner)
    {
        return Some(cmd);
    }

    let text = card.get("text").and_then(|v| v.as_str())?;
    for line in text.lines() {
        let line = strip_markdown_list_prefix(line);
        let Some(rest) = line.trim_start().strip_prefix("CMD:") else {
            continue;
        };
        let cmd = rest.trim();
        if !looks_like_placeholder(cmd) {
            return Some(cmd.to_string());
        }
    }
    None
}

fn looks_like_tradeoff_text(value: &str) -> bool {
    let s = value.trim().to_ascii_lowercase();
    if s.is_empty() {
        return false;
    }
    s.contains(" vs ")
        || s.contains(" versus ")
        || s.contains("tradeoff")
        || s.contains("trade-off")
        || s.contains("a/b")
}

fn card_label(card: &Value) -> String {
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

fn shorten(s: &str, max_chars: usize) -> String {
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

fn ref_card(card_id: &str) -> EngineRef {
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

fn signal_at(
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

fn action_at(
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

fn ms_per_day() -> i64 {
    86_400_000
}

fn max_ts_ms(cards: &[Value], trace_entries: &[Value]) -> i64 {
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

fn extract_stale_after_ms_from_test_card(card: &Value) -> Option<i64> {
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

fn extract_stale_after_days_from_test_card(card: &Value) -> Option<i64> {
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

fn refs_from_ids(ids: &[String], max: usize) -> Vec<EngineRef> {
    let mut out = Vec::new();
    for id in ids.iter().take(max) {
        out.push(ref_card(id));
    }
    out
}

fn edge_triplet(edge: &Value) -> Option<(&str, &str, &str)> {
    let from = edge.get("from").and_then(|v| v.as_str())?;
    let rel = edge.get("rel").and_then(|v| v.as_str())?;
    let to = edge.get("to").and_then(|v| v.as_str())?;
    Some((from, rel, to))
}

fn trace_has_progress_signal(trace_entries: &[Value]) -> bool {
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

fn card_has_tag(card: &Value, tag: &str) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter().any(|t| t.as_str() == Some(tag))
}

fn card_status_is_active_for_discipline(card: &Value) -> bool {
    let status = card
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open")
        .trim();
    !(status.eq_ignore_ascii_case("closed")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("resolved"))
}

fn card_is_draft_like(card: &Value) -> bool {
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
