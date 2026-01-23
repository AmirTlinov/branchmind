#![forbid(unsafe_code)]

use crate::card_value_anchor_tags;
use crate::support::ai::suggest_call;
use crate::support::proof::looks_like_bare_url;
use crate::{LANE_TAG_AGENT_PREFIX, LANE_TAG_SHARED, PIN_TAG, VIS_TAG_DRAFT};
use serde_json::{Value, json};

pub(crate) const REASONING_ENGINE_VERSION: &str = "v0.5";

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

pub(crate) fn derive_reasoning_engine(
    scope: EngineScope<'_>,
    cards: &[Value],
    edges: &[Value],
    trace_entries: &[Value],
    limits: EngineLimits,
) -> Option<Value> {
    if limits.signals_limit == 0 && limits.actions_limit == 0 {
        return None;
    }

    let mut signals: Vec<EngineSignal> = Vec::new();
    let mut actions: Vec<EngineAction> = Vec::new();
    let reference_ts_ms = max_ts_ms(cards, trace_entries);

    // Build id -> card lookup (small; deterministic via BTreeMap key ordering).
    let mut by_id = std::collections::BTreeMap::<String, &Value>::new();
    for card in cards {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        by_id.insert(id.to_string(), card);
    }

    // Build incoming adjacency for supports/blocks.
    let mut incoming_supports = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut incoming_blocks = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut outgoing_supports = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut outgoing_blocks = std::collections::BTreeMap::<String, Vec<String>>::new();
    for edge in edges {
        let Some((from, rel, to)) = edge_triplet(edge) else {
            continue;
        };
        if !by_id.contains_key(from) || !by_id.contains_key(to) {
            continue;
        }
        match rel {
            "supports" => {
                incoming_supports
                    .entry(to.to_string())
                    .or_default()
                    .push(from.to_string());
                outgoing_supports
                    .entry(from.to_string())
                    .or_default()
                    .push(to.to_string());
            }
            "blocks" => {
                incoming_blocks
                    .entry(to.to_string())
                    .or_default()
                    .push(from.to_string());
                outgoing_blocks
                    .entry(from.to_string())
                    .or_default()
                    .push(to.to_string());
            }
            _ => {}
        }
    }
    for list in incoming_supports.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in incoming_blocks.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in outgoing_supports.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in outgoing_blocks.values_mut() {
        list.sort();
        list.dedup();
    }

    // ===== BM2: Evidence strength scoring (deterministic, slice-only) =====
    let mut evidence_scores = std::collections::BTreeMap::<String, u8>::new();
    for (id, card) in by_id.iter() {
        if card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
            continue;
        }
        let score = evidence_strength_score(card, &outgoing_supports, &outgoing_blocks, &by_id);
        evidence_scores.insert(id.to_string(), score);
    }

    // ===== Draft hygiene: draft decisions should be promoted into canon =====
    // Motivation: drafts are intentionally low-visibility, but decisions are knowledge anchors and
    // should not silently remain stuck as `v:draft` forever.
    //
    // Deterministic: derived from the returned slice only (no extra store reads).
    let recent_window_ms = 14i64.saturating_mul(ms_per_day());
    let mut lane_decisions: Vec<&Value> = by_id
        .values()
        .filter(|card| card.get("type").and_then(|v| v.as_str()) == Some("decision"))
        .filter(|card| card_is_draft_like(card))
        .copied()
        .collect();
    lane_decisions.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for decision in lane_decisions.iter().take(8) {
        let Some(decision_id) = decision.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if decision_id.trim().starts_with("CARD-PUB-") {
            continue;
        }
        let decision_ts_ms = decision
            .get("last_ts_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let is_pinned = card_has_tag(decision, PIN_TAG);
        let is_recent = reference_ts_ms == 0
            || decision_ts_ms >= reference_ts_ms.saturating_sub(recent_window_ms);
        if !is_pinned && !is_recent {
            continue;
        }

        // Best-effort slice-only check: if the deterministic published id is present in this slice,
        // do not emit a redundant publish suggestion.
        let published_id = format!("CARD-PUB-{}", decision_id.trim());
        if by_id.contains_key(published_id.as_str()) {
            continue;
        }

        let label = shorten(&card_label(decision), 64);
        signals.push(signal_at(
            "BM_LANE_DECISION_NOT_PUBLISHED",
            "warning",
            format!("Decision is draft-scoped (v:draft) and not promoted to canon: {label}"),
            vec![ref_card(decision_id)],
            decision_ts_ms,
        ));

        actions.push(action_at(
            "publish_decision",
            "medium",
            format!("Promote decision to canon (pinned): {label}"),
            Some(
                "Draft hygiene: promote decisions so they become stable resume anchors across sessions."
                    .to_string(),
            ),
            vec![ref_card(decision_id)],
            vec![suggest_call(
                "think_publish",
                "Promote this decision into canon (deterministic published copy).",
                "medium",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card_id": decision_id,
                    "pin": true
                }),
            )],
            decision_ts_ms,
        ));
    }

    // ===== BM4: Blind spot detection (hypothesis without tests/evidence) =====
    let mut hypotheses: Vec<&Value> = by_id
        .values()
        .filter(|card| card.get("type").and_then(|v| v.as_str()) == Some("hypothesis"))
        // Treat hypotheses as active unless explicitly closed. This prevents bypassing
        // discipline checks via status drift (e.g. "accepted", "done").
        .filter(|card| card_status_is_active_for_discipline(card))
        .copied()
        .collect();
    hypotheses.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for hypo in hypotheses.iter().take(12) {
        let Some(hypo_id) = hypo.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let hypo_ts_ms = hypo.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let supporters = incoming_supports.get(hypo_id);
        let mut supporting_tests: Vec<&Value> = Vec::new();
        let mut direct_evidence = false;

        if let Some(ids) = supporters {
            for from_id in ids {
                let Some(from_card) = by_id.get(from_id).copied() else {
                    continue;
                };
                match from_card.get("type").and_then(|v| v.as_str()) {
                    Some("test") => supporting_tests.push(from_card),
                    Some("evidence") => direct_evidence = true,
                    _ => {}
                }
            }
        }

        let mut indirect_evidence = false;
        for test in &supporting_tests {
            let Some(test_id) = test.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(ids) = incoming_supports.get(test_id) {
                for from_id in ids {
                    let Some(from_card) = by_id.get(from_id).copied() else {
                        continue;
                    };
                    if from_card.get("type").and_then(|v| v.as_str()) == Some("evidence") {
                        indirect_evidence = true;
                        break;
                    }
                }
            }
            if indirect_evidence {
                break;
            }
        }

        if supporting_tests.is_empty() {
            let label = shorten(&card_label(hypo), 64);
            signals.push(signal_at(
                "BM4_HYPOTHESIS_NO_TEST",
                "high",
                format!("Hypothesis has no linked tests: {label}"),
                vec![ref_card(hypo_id)],
                hypo_ts_ms,
            ));
            let calls = vec![suggest_call(
                "think_card",
                "Create a test stub that supports this hypothesis (fill command later).",
                "high",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "test",
                        "title": format!("Test: {label}"),
                        "text": "Define the smallest runnable check for this hypothesis.",
                        "status": "open",
                        "tags": ["bm4"]
                    },
                    "supports": [hypo_id]
                }),
            )];
            actions.push(action_at(
                "add_test_stub",
                "high",
                format!("Add a test for: {label}"),
                Some("BM4: no linked tests found in current slice.".to_string()),
                vec![ref_card(hypo_id)],
                calls,
                hypo_ts_ms,
            ));
        } else if !direct_evidence && !indirect_evidence {
            let label = shorten(&card_label(hypo), 64);
            signals.push(signal_at(
                "BM4_HYPOTHESIS_NO_EVIDENCE",
                "warning",
                format!("Hypothesis has tests but no linked evidence (in slice): {label}"),
                vec![ref_card(hypo_id)],
                hypo_ts_ms,
            ));
        }
    }

    // ===== BM1: Contradiction detection (supports + blocks on same target) =====
    // Deterministic heuristic: if a card has both incoming supports and incoming blocks edges
    // (within the returned slice), surface it as a contradiction that needs a disambiguating test
    // or an explicit decision.
    let mut contradiction_targets: Vec<&Value> = by_id
        .values()
        .filter(|card| {
            let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
            matches!(ty, "hypothesis" | "test" | "decision")
        })
        .filter(|card| {
            card.get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("open")
                == "open"
        })
        .copied()
        .collect();
    contradiction_targets.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for target in contradiction_targets.iter().take(10) {
        let Some(target_id) = target.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let supports = incoming_supports
            .get(target_id)
            .cloned()
            .unwrap_or_default();
        let blocks = incoming_blocks.get(target_id).cloned().unwrap_or_default();
        if supports.is_empty() || blocks.is_empty() {
            continue;
        }

        let target_ts_ms = target
            .get("last_ts_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let label = shorten(&card_label(target), 64);

        let mut refs = vec![ref_card(target_id)];
        refs.extend(refs_from_ids(&supports, 2));
        refs.extend(refs_from_ids(&blocks, 2));

        signals.push(signal_at(
            "BM1_CONTRADICTION_SUPPORTS_BLOCKS",
            "high",
            format!("Contradiction detected (supports vs blocks) for: {label}"),
            refs.clone(),
            target_ts_ms,
        ));

        let calls = vec![
            suggest_call(
                "think_playbook",
                "Load a deterministic contradiction-resolution playbook.",
                "medium",
                json!({ "workspace": scope.workspace, "name": "contradiction" }),
            ),
            suggest_call(
                "think_card",
                "Write a focused question that forces a decisive test or decision.",
                "high",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "question",
                        "title": format!("Resolve contradiction: {label}"),
                        "text": "List strongest evidence on both sides, then define the smallest decisive test.",
                        "status": "open",
                        "tags": ["bm1", "contradiction"],
                        "meta": { "about": { "kind": "card", "id": target_id } }
                    }
                }),
            ),
        ];
        actions.push(action_at(
            "resolve_contradiction",
            "high",
            format!("Resolve contradiction: {label}"),
            Some("BM1: both supports and blocks edges exist in the current slice.".to_string()),
            refs,
            calls,
            target_ts_ms,
        ));
    }

    // ===== BM2: Evidence strength (weak pinned evidence is actionable debt) =====
    // Keep output intentionally small: at most 2 weak evidence warnings.
    let mut pinned_decision_ids = std::collections::BTreeSet::<String>::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("decision") {
            continue;
        }
        if card_has_tag(card, PIN_TAG)
            && let Some(id) = card.get("id").and_then(|v| v.as_str())
        {
            pinned_decision_ids.insert(id.to_string());
        }
    }

    let mut weak_evidence: Vec<(&Value, u8)> = Vec::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let score = evidence_scores.get(id).copied().unwrap_or(0);
        if score >= 60 {
            continue;
        }

        let mut important = card_has_tag(card, PIN_TAG);
        if !important {
            if let Some(targets) = outgoing_supports.get(id) {
                important |= targets.iter().any(|t| pinned_decision_ids.contains(t));
            }
            if let Some(targets) = outgoing_blocks.get(id) {
                important |= targets.iter().any(|t| pinned_decision_ids.contains(t));
            }
        }
        if !important {
            continue;
        }

        weak_evidence.push((card, score));
    }
    weak_evidence.sort_by(|(a, ascore), (b, bscore)| {
        ascore.cmp(bscore).then_with(|| {
            let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            b_ts.cmp(&a_ts).then_with(|| {
                let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                a_id.cmp(b_id)
            })
        })
    });

    for (card, score) in weak_evidence.into_iter().take(2) {
        let Some(evidence_id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let receipts = evidence_receipts(card);
        let mut missing = Vec::<&str>::new();
        if !receipts.cmd {
            missing.push("CMD");
        }
        if !receipts.link {
            missing.push("LINK");
        }
        let missing = if missing.is_empty() {
            "receipts".to_string()
        } else {
            missing.join("+")
        };
        let label = shorten(&card_label(card), 64);
        signals.push(signal_at(
            "BM2_EVIDENCE_WEAK",
            "warning",
            format!("Evidence is weak (score {score}/100; missing {missing}): {label}"),
            vec![ref_card(evidence_id)],
            ts_ms,
        ));
    }

    // ===== BM3: Confidence propagation (what is actually proven?) =====
    // Deterministic, slice-only, shallow depth to avoid cycles.
    let mut memo = std::collections::BTreeMap::<String, f64>::new();
    let mut stack = std::collections::BTreeSet::<String>::new();

    let confidence_ctx = ConfidenceContext {
        by_id: &by_id,
        incoming_supports: &incoming_supports,
        incoming_blocks: &incoming_blocks,
        evidence_scores: &evidence_scores,
    };

    #[derive(Clone, Debug)]
    struct ConfidenceCandidate<'a> {
        id: &'a str,
        card: &'a Value,
        confidence: f64,
    }

    let mut pinned_decisions: Vec<ConfidenceCandidate<'_>> = Vec::new();
    let mut open_hypotheses: Vec<ConfidenceCandidate<'_>> = Vec::new();

    for card in by_id.values() {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ty {
            "decision" => {
                if !card_has_tag(card, PIN_TAG) {
                    continue;
                }
                let c = confidence_for_id(id, 3, &confidence_ctx, &mut memo, &mut stack);
                pinned_decisions.push(ConfidenceCandidate {
                    id,
                    card,
                    confidence: c,
                });
            }
            "hypothesis" => {
                if card
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("open")
                    != "open"
                {
                    continue;
                }
                let c = confidence_for_id(id, 3, &confidence_ctx, &mut memo, &mut stack);
                open_hypotheses.push(ConfidenceCandidate {
                    id,
                    card,
                    confidence: c,
                });
            }
            _ => {}
        }
    }

    pinned_decisions.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_ts = a
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let b_ts = b
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                b_ts.cmp(&a_ts).then_with(|| a.id.cmp(b.id))
            })
    });
    open_hypotheses.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_ts = a
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let b_ts = b
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                b_ts.cmp(&a_ts).then_with(|| a.id.cmp(b.id))
            })
    });

    if let Some(worst) = pinned_decisions.first() {
        let threshold = 0.55;
        if worst.confidence <= threshold {
            let ts_ms = worst
                .card
                .get("last_ts_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let supports = incoming_supports
                .get(worst.id)
                .map(|v| v.len())
                .unwrap_or(0);
            let blocks = incoming_blocks.get(worst.id).map(|v| v.len()).unwrap_or(0);
            let label = shorten(&card_label(worst.card), 64);
            signals.push(signal_at(
                "BM3_DECISION_LOW_CONFIDENCE",
                "warning",
                format!(
                    "Low confidence for pinned decision (~{:.2}); supports={} blocks={}: {label}",
                    worst.confidence, supports, blocks
                ),
                vec![ref_card(worst.id)],
                ts_ms,
            ));

            actions.push(action_at(
                "use_playbook",
                "medium",
                format!("Design a decisive experiment for: {label}"),
                Some(
                    "BM9: low-confidence anchors benefit from a single decisive experiment."
                        .to_string(),
                ),
                vec![ref_card(worst.id)],
                vec![suggest_call(
                    "think_playbook",
                    "Get a deterministic experiment playbook skeleton.",
                    "medium",
                    json!({ "workspace": scope.workspace, "name": "experiment" }),
                )],
                ts_ms,
            ));
        }
    } else if let Some(worst) = open_hypotheses.first() {
        let threshold = 0.45;
        if worst.confidence <= threshold {
            let ts_ms = worst
                .card
                .get("last_ts_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let supports = incoming_supports
                .get(worst.id)
                .map(|v| v.len())
                .unwrap_or(0);
            let blocks = incoming_blocks.get(worst.id).map(|v| v.len()).unwrap_or(0);
            let label = shorten(&card_label(worst.card), 64);
            signals.push(signal_at(
                "BM3_HYPOTHESIS_LOW_CONFIDENCE",
                "info",
                format!(
                    "Low confidence for hypothesis (~{:.2}); supports={} blocks={}: {label}",
                    worst.confidence, supports, blocks
                ),
                vec![ref_card(worst.id)],
                ts_ms,
            ));
        }
    }

    // ===== BM6: Assumption surfacing (cascade when assumptions change) =====
    // Heuristic: treat cards tagged `assumption` as first-class assumptions.
    // When a non-open assumption still supports active cards (open/pinned), surface it.
    #[derive(Clone, Debug)]
    struct AssumptionIssue {
        id: String,
        title: String,
        status: String,
        ts_ms: i64,
        impacted: Vec<String>,
    }

    let mut assumption_issues = Vec::<AssumptionIssue>::new();
    for card in by_id.values() {
        let tags = card_tags_lower(card);
        if !tags.iter().any(|t| t == "assumption") {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let status = card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            .trim()
            .to_string();
        if status.eq_ignore_ascii_case("open") {
            continue;
        }

        let mut impacted = Vec::<String>::new();
        if let Some(targets) = outgoing_supports.get(id) {
            for to in targets {
                let Some(target) = by_id.get(to).copied() else {
                    continue;
                };
                let ty = target.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if !matches!(ty, "decision" | "hypothesis") {
                    continue;
                }
                let active = card_has_tag(target, PIN_TAG)
                    || target
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("open")
                        .eq_ignore_ascii_case("open");
                if active {
                    impacted.push(to.to_string());
                }
            }
        }
        impacted.sort();
        impacted.dedup();
        if impacted.is_empty() {
            continue;
        }

        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let title = shorten(&card_label(card), 64);
        assumption_issues.push(AssumptionIssue {
            id: id.to_string(),
            title,
            status,
            ts_ms,
            impacted,
        });
    }
    assumption_issues.sort_by(|a, b| {
        b.ts_ms
            .cmp(&a.ts_ms)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.title.cmp(&b.title))
    });

    if let Some(issue) = assumption_issues.into_iter().next() {
        let refs = {
            let mut refs = vec![ref_card(issue.id.as_str())];
            refs.extend(refs_from_ids(&issue.impacted, 4));
            refs
        };
        signals.push(signal_at(
            "BM6_ASSUMPTION_NOT_OPEN_BUT_USED",
            "warning",
            format!(
                "Assumption is not open (status={}) but still supports {} active cards: {}",
                issue.status,
                issue.impacted.len(),
                issue.title
            ),
            refs.clone(),
            issue.ts_ms,
        ));
        actions.push(action_at(
            "recheck_assumption",
            "medium",
            format!("Recheck assumption cascade: {}", issue.title),
            Some(
                "BM6: when an assumption changes, dependent cards must be re-evaluated."
                    .to_string(),
            ),
            refs,
            vec![suggest_call(
                "think_card",
                "Create an update card listing impacted decisions/hypotheses and the next decisive test.",
                "medium",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "update",
                        "title": format!("Assumption changed: {}", issue.title),
                        "text": "List impacted decisions/hypotheses, then define ONE decisive experiment to restore confidence.",
                        "status": "open",
                        "tags": ["bm6", "assumption"]
                    },
                    "supports": [issue.id]
                }),
            )],
            issue.ts_ms,
        ));
    }

    // ===== BM9: Reasoning patterns (deterministic playbooks) =====
    // Detect classic A vs B framing and suggest criteria matrix as a low-priority backup.
    let mut tradeoff_candidate: Option<(&str, i64)> = None;
    for card in by_id.values() {
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(ty, "question" | "decision") {
            continue;
        }
        if card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            != "open"
        {
            continue;
        }

        let mut matched = false;
        if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
            matched |= looks_like_tradeoff_text(title);
        }
        if !matched && let Some(text) = card.get("text").and_then(|v| v.as_str()) {
            matched |= looks_like_tradeoff_text(text);
        }
        if !matched {
            continue;
        }

        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let replace = match tradeoff_candidate {
            None => true,
            Some((_prev_id, prev_ts)) => ts_ms > prev_ts || (ts_ms == prev_ts && id < _prev_id),
        };
        if replace {
            tradeoff_candidate = Some((id, ts_ms));
        }
    }

    if let Some((id, ts_ms)) = tradeoff_candidate {
        actions.push(action_at(
            "use_playbook",
            "low",
            "Load criteria matrix playbook (A vs B)".to_string(),
            Some("BM9: tradeoffs are cheaper with a criteria matrix.".to_string()),
            vec![ref_card(id)],
            vec![suggest_call(
                "think_playbook",
                "Get a deterministic criteria-matrix playbook skeleton.",
                "low",
                json!({ "workspace": scope.workspace, "name": "criteria_matrix" }),
            )],
            ts_ms,
        ));
    }

    // ===== BM5: Executable tests (next runnable test) =====
    // ===== BM8: Time-decay (stale evidence → recommend re-run) =====
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RunnableEvidenceState {
        Missing,
        Stale,
        Fresh,
    }

    #[derive(Clone, Debug)]
    struct RunnableTestCandidate {
        test_ts_ms: i64,
        test_id: String,
        cmd: String,
        state: RunnableEvidenceState,
        evidence_latest_ts_ms: Option<i64>,
    }

    let mut runnable_tests: Vec<RunnableTestCandidate> = Vec::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("test") {
            continue;
        }
        if card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            != "open"
        {
            continue;
        }
        let Some(test_id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(cmd) = extract_cmd_from_test_card(card) else {
            continue;
        };
        let test_ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);

        let mut evidence_latest_ts_ms: Option<i64> = None;
        if let Some(supporters) = incoming_supports.get(test_id) {
            for from_id in supporters {
                let Some(from_card) = by_id.get(from_id).copied() else {
                    continue;
                };
                if from_card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
                    continue;
                }
                let ts = from_card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                evidence_latest_ts_ms = Some(evidence_latest_ts_ms.unwrap_or(0).max(ts));
            }
        }

        let stale_after_ms = extract_stale_after_ms_from_test_card(card).unwrap_or_else(|| {
            let days = extract_stale_after_days_from_test_card(card).unwrap_or(30);
            let days = days.clamp(0, 3650);
            days.saturating_mul(ms_per_day())
        });

        let state = match evidence_latest_ts_ms {
            None => RunnableEvidenceState::Missing,
            Some(evidence_ts_ms) => {
                if reference_ts_ms > evidence_ts_ms.saturating_add(stale_after_ms) {
                    RunnableEvidenceState::Stale
                } else {
                    RunnableEvidenceState::Fresh
                }
            }
        };

        runnable_tests.push(RunnableTestCandidate {
            test_ts_ms,
            test_id: test_id.to_string(),
            cmd,
            state,
            evidence_latest_ts_ms,
        });
    }

    runnable_tests.sort_by(|a, b| {
        let state_rank = |s: RunnableEvidenceState| match s {
            RunnableEvidenceState::Missing => 0,
            RunnableEvidenceState::Stale => 1,
            RunnableEvidenceState::Fresh => 2,
        };
        state_rank(a.state)
            .cmp(&state_rank(b.state))
            .then_with(|| b.test_ts_ms.cmp(&a.test_ts_ms))
            .then_with(|| a.test_id.cmp(&b.test_id))
    });

    let runnable_total = runnable_tests.len();
    if let Some(best) = runnable_tests
        .iter()
        .find(|c| c.state != RunnableEvidenceState::Fresh)
    {
        let label = by_id
            .get(best.test_id.as_str())
            .map(|c| shorten(&card_label(c), 64))
            .unwrap_or_else(|| best.test_id.to_string());
        let cmd_short = shorten(best.cmd.as_str(), 96);

        if best.state == RunnableEvidenceState::Stale {
            let age_ms = best
                .evidence_latest_ts_ms
                .map(|ts| reference_ts_ms.saturating_sub(ts))
                .unwrap_or(0);
            let age_days = if ms_per_day() == 0 {
                0
            } else {
                age_ms / ms_per_day()
            };
            signals.push(signal_at(
                "BM8_EVIDENCE_STALE",
                "warning",
                format!("Evidence looks stale for runnable test: {label} (age≈{age_days}d)"),
                vec![ref_card(best.test_id.as_str())],
                best.test_ts_ms,
            ));
        }

        let (priority, why) = match best.state {
            RunnableEvidenceState::Missing => (
                "high",
                Some("BM5: runnable test has no linked evidence in current slice.".to_string()),
            ),
            RunnableEvidenceState::Stale => (
                "medium",
                Some(
                    "BM8: runnable test has evidence, but it looks stale in this slice."
                        .to_string(),
                ),
            ),
            RunnableEvidenceState::Fresh => ("low", None),
        };

        let calls = vec![suggest_call(
            "think_card",
            "After running the test, capture evidence and link it to the test card.",
            "high",
            json!({
                "workspace": scope.workspace,
                "branch": scope.branch,
                "trace_doc": scope.trace_doc,
                "graph_doc": scope.graph_doc,
                "card": {
                    "type": "evidence",
                    "title": format!("Evidence: {label}"),
                    "text": "Paste the command output and an artifact link; keep it factual.",
                    "status": "open",
                    "tags": ["bm5"],
                    "meta": { "run": { "cmd": best.cmd.as_str() } }
                },
                "supports": [best.test_id.as_str()]
            }),
        )];

        actions.push(action_at(
            "run_test",
            priority,
            format!("Run test: {label} ({cmd_short})"),
            why,
            vec![ref_card(best.test_id.as_str())],
            calls,
            best.test_ts_ms,
        ));
    } else if runnable_total > 0 {
        signals.push(signal_at(
            "BM5_RUNNABLE_TESTS_FRESH",
            "info",
            format!("{runnable_total} runnable tests detected; evidence appears fresh in slice."),
            Vec::new(),
            reference_ts_ms,
        ));
    }

    // ===== BM10: Meta-reasoning hooks (stuck + bias risk) =====
    let has_progress = trace_has_progress_signal(trace_entries);
    let mut recent_think_cards = 0usize;
    for entry in trace_entries.iter().rev().take(12) {
        if entry.get("kind").and_then(|v| v.as_str()) == Some("note")
            && entry.get("format").and_then(|v| v.as_str()) == Some("think_card")
        {
            recent_think_cards += 1;
        }
    }

    if !has_progress && recent_think_cards >= 6 {
        signals.push(signal_at(
            "BM10_STUCK_NO_EVIDENCE",
            "warning",
            "No recent evidence captured in trace slice; consider pivoting to the smallest runnable test.".to_string(),
            Vec::new(),
            reference_ts_ms,
        ));
        actions.push(action_at(
            "use_playbook",
            "medium",
            "Load debug playbook (reframe → test → evidence)".to_string(),
            Some(
                "BM10: trace suggests low progress; a structured reset is cheaper than spinning."
                    .to_string(),
            ),
            Vec::new(),
            vec![
                suggest_call(
                    "think_playbook",
                    "Get a deterministic debug playbook skeleton.",
                    "medium",
                    json!({ "workspace": scope.workspace, "name": "debug" }),
                ),
                suggest_call(
                    "think_playbook",
                    "If you're looping, load the breakthrough playbook (inversion → 10x lever → decisive test).",
                    "low",
                    json!({ "workspace": scope.workspace, "name": "breakthrough" }),
                ),
            ],
            reference_ts_ms,
        ));
    }

    // ===== BM7: Counter-argument generation (steelman) =====
    // Find a concrete target that has supports but no blocks edges in this slice.
    // This is both a bias alert (BM10) and a prompt to add a counter-position (BM7).
    #[derive(Clone, Debug)]
    struct CounterTarget<'a> {
        id: &'a str,
        card: &'a Value,
        ts_ms: i64,
        supports: usize,
    }

    let mut counter_targets = Vec::<CounterTarget<'_>>::new();
    for card in by_id.values() {
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(ty, "hypothesis" | "decision") {
            continue;
        }
        // Counter-hypotheses are themselves the "blocks" side of the dialectic. Requiring a
        // counter-position for a counter-position leads to infinite regress, so we treat cards
        // tagged as `counter` as exempt from BM10.
        if card_has_tag(card, "counter") {
            continue;
        }
        if !card_status_is_active_for_discipline(card) {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let supports = incoming_supports.get(id).map(|v| v.len()).unwrap_or(0);
        let blocks = incoming_blocks.get(id).map(|v| v.len()).unwrap_or(0);
        if supports == 0 || blocks > 0 {
            continue;
        }
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        counter_targets.push(CounterTarget {
            id,
            card,
            ts_ms,
            supports,
        });
    }
    counter_targets.sort_by(|a, b| {
        b.supports
            .cmp(&a.supports)
            .then_with(|| b.ts_ms.cmp(&a.ts_ms))
            .then_with(|| a.id.cmp(b.id))
    });

    if let Some(target) = counter_targets.into_iter().next() {
        let label = shorten(&card_label(target.card), 64);

        signals.push(signal_at(
            "BM10_NO_COUNTER_EDGES",
            "info",
            format!(
                "Card has supports but no blocks edges (in slice); add a counter-position: {label}"
            ),
            vec![ref_card(target.id)],
            target.ts_ms.max(reference_ts_ms),
        ));

        actions.push(action_at(
            "add_counter_hypothesis",
            "medium",
            format!("Steelman a counter-hypothesis for: {label}"),
            Some(
                "BM7: counter-arguments reduce confirmation bias and sharpen the next decisive test."
                    .to_string(),
            ),
            vec![ref_card(target.id)],
            {
                let mut tags = vec!["bm7".to_string(), "counter".to_string()];
                tags.extend(card_value_anchor_tags(target.card));
                tags = bm_core::graph::normalize_tags(&tags).unwrap_or(tags);

                vec![
                    suggest_call(
                        "think_playbook",
                        "Load a short skeptic loop (counter-hypothesis → falsifier → stop criteria).",
                        "low",
                        json!({ "workspace": scope.workspace, "name": "skeptic" }),
                    ),
                    suggest_call(
                        "think_card",
                        "Write the strongest opposite hypothesis + cheapest falsifier + stop criteria.",
                        "medium",
                        json!({
                            "workspace": scope.workspace,
                            "branch": scope.branch,
                            "trace_doc": scope.trace_doc,
                            "graph_doc": scope.graph_doc,
                            "card": {
                                "type": "hypothesis",
                                "title": format!("Counter-hypothesis: {label}"),
                                "text": "Steelman the opposite case.\n- Minimal falsifying test: (what would disprove this quickly?)\n- Stop criteria (time/budget/signal): (when do we stop debating?)",
                                "status": "open",
                                "tags": tags
                            },
                            "blocks": [target.id]
                        }),
                    ),
                ]
            },
            target.ts_ms.max(reference_ts_ms),
        ));
    }

    // Deterministic ordering + budgets.
    signals.sort_by(|a, b| {
        b.severity_rank
            .cmp(&a.severity_rank)
            .then_with(|| b.sort_ts_ms.cmp(&a.sort_ts_ms))
            .then_with(|| a.code.cmp(b.code))
            .then_with(|| a.message.cmp(&b.message))
    });
    actions.sort_by(|a, b| {
        b.priority_rank
            .cmp(&a.priority_rank)
            .then_with(|| b.sort_ts_ms.cmp(&a.sort_ts_ms))
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.title.cmp(&b.title))
    });

    let signals_total = signals.len();
    let actions_total = actions.len();
    let mut truncated = false;

    let signals_out = if limits.signals_limit == 0 {
        Vec::new()
    } else {
        let limit = limits.signals_limit.max(1);
        if signals.len() > limit {
            truncated = true;
        }
        signals
            .into_iter()
            .take(limit)
            .map(|s| {
                json!({
                    "code": s.code,
                    "severity": s.severity,
                    "message": s.message,
                    "refs": s.refs.into_iter().map(|r| json!({"kind": r.kind, "id": r.id})).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    };

    let actions_out = if limits.actions_limit == 0 {
        Vec::new()
    } else {
        let limit = limits.actions_limit.max(1);
        if actions.len() > limit {
            truncated = true;
        }
        actions
            .into_iter()
            .take(limit)
            .map(|a| {
                json!({
                    "kind": a.kind,
                    "priority": a.priority,
                    "title": a.title,
                    "why": a.why,
                    "refs": a.refs.into_iter().map(|r| json!({"kind": r.kind, "id": r.id})).collect::<Vec<_>>(),
                    "calls": a.calls
                })
            })
            .collect::<Vec<_>>()
    };

    if signals_out.is_empty() && actions_out.is_empty() {
        return None;
    }

    Some(json!({
        "version": REASONING_ENGINE_VERSION,
        "signals_total": signals_total,
        "actions_total": actions_total,
        "signals": signals_out,
        "actions": actions_out,
        "truncated": truncated
    }))
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

fn merge_engine_arrays(
    primary: Vec<Value>,
    secondary: Vec<Value>,
    limit: usize,
    key_fn: fn(&Value) -> Option<String>,
) -> (Vec<Value>, bool) {
    if limit == 0 {
        return (Vec::new(), false);
    }

    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut truncated = false;

    for item in primary.into_iter().chain(secondary.into_iter()) {
        if out.len() >= limit {
            truncated = true;
            break;
        }
        let Some(key) = key_fn(&item) else {
            // Non-conforming items are kept (deterministically) and don't participate in dedupe.
            out.push(item);
            continue;
        };
        if seen.insert(key) {
            out.push(item);
        }
    }

    (out, truncated)
}

fn signal_key(item: &Value) -> Option<String> {
    let code = item.get("code").and_then(|v| v.as_str())?;
    let message = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
    Some(format!("{code}\n{message}"))
}

fn action_key(item: &Value) -> Option<String> {
    let kind = item.get("kind").and_then(|v| v.as_str())?;
    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    Some(format!("{kind}\n{title}"))
}

fn step_selector_from_step_tag(step_tag: &str) -> Option<String> {
    let step = step_tag
        .strip_prefix("step:")
        .unwrap_or(step_tag)
        .trim()
        .to_string();
    if step.is_empty() {
        return None;
    }
    Some(step.to_ascii_uppercase())
}

fn apply_step_scope_to_engine_calls(
    engine_obj: &mut serde_json::Map<String, Value>,
    step_tag: &str,
) {
    let Some(step_selector) = step_selector_from_step_tag(step_tag) else {
        return;
    };
    let Some(actions) = engine_obj.get_mut("actions").and_then(|v| v.as_array_mut()) else {
        return;
    };

    for action in actions {
        let Some(action_obj) = action.as_object_mut() else {
            continue;
        };
        let Some(calls) = action_obj.get_mut("calls").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for call in calls {
            let Some(call_obj) = call.as_object_mut() else {
                continue;
            };
            let target = call_obj.get("target").and_then(|v| v.as_str());
            if !matches!(target, Some("think_card" | "think_pipeline")) {
                continue;
            }
            let Some(params) = call_obj.get_mut("params").and_then(|v| v.as_object_mut()) else {
                continue;
            };
            params
                .entry("step".to_string())
                .or_insert_with(|| Value::String(step_selector.clone()));
        }
    }
}

pub(crate) fn derive_reasoning_engine_step_aware(
    scope: EngineScope<'_>,
    cards: &[Value],
    edges: &[Value],
    trace_entries: &[Value],
    focus_step_tag: Option<&str>,
    limits: EngineLimits,
) -> Option<Value> {
    let step_tag = focus_step_tag.map(str::trim).filter(|t| !t.is_empty());
    if step_tag.is_none() {
        return derive_reasoning_engine(scope, cards, edges, trace_entries, limits);
    }
    let step_tag = step_tag.unwrap();

    let step_cards = cards
        .iter()
        .filter(|card| card_has_tag(card, step_tag))
        .cloned()
        .collect::<Vec<_>>();
    if step_cards.is_empty() {
        return derive_reasoning_engine(scope, cards, edges, trace_entries, limits);
    }

    let mut step_ids = std::collections::BTreeSet::<String>::new();
    for card in &step_cards {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            step_ids.insert(id.to_string());
        }
    }

    let step_edges = edges
        .iter()
        .filter(|edge| {
            let from = edge.get("from").and_then(|v| v.as_str());
            let to = edge.get("to").and_then(|v| v.as_str());
            from.is_some_and(|id| step_ids.contains(id))
                && to.is_some_and(|id| step_ids.contains(id))
        })
        .cloned()
        .collect::<Vec<_>>();

    let step_engine =
        derive_reasoning_engine(scope, &step_cards, &step_edges, trace_entries, limits);
    let global_engine = derive_reasoning_engine(scope, cards, edges, trace_entries, limits);

    let (mut global, step) = match (global_engine, step_engine) {
        (None, None) => return None,
        (Some(global), None) => return Some(global),
        (None, Some(step)) => return Some(step),
        (Some(global), Some(step)) => (global, step),
    };

    let Some(global_obj) = global.as_object_mut() else {
        return Some(global);
    };
    let step_obj = step.as_object().cloned().unwrap_or_default();

    let limit_signals = limits.signals_limit;
    let limit_actions = limits.actions_limit;

    let step_signals = step_obj
        .get("signals")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let global_signals = global_obj
        .get("signals")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let step_actions = step_obj
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let global_actions = global_obj
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let (merged_signals, truncated_signals) =
        merge_engine_arrays(step_signals, global_signals, limit_signals, signal_key);
    let (merged_actions, truncated_actions) =
        merge_engine_arrays(step_actions, global_actions, limit_actions, action_key);

    global_obj.insert("signals".to_string(), Value::Array(merged_signals));
    global_obj.insert("actions".to_string(), Value::Array(merged_actions));
    global_obj.insert(
        "truncated".to_string(),
        Value::Bool(
            global_obj
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || truncated_signals
                || truncated_actions,
        ),
    );
    global_obj.insert("mode".to_string(), Value::String("step_aware".to_string()));
    global_obj.insert("step_tag".to_string(), Value::String(step_tag.to_string()));
    apply_step_scope_to_engine_calls(global_obj, step_tag);

    Some(Value::Object(global_obj.clone()))
}

pub(crate) fn filter_engine_to_cards(engine: &mut Value, cards: &[Value]) {
    let Some(obj) = engine.as_object_mut() else {
        return;
    };
    let _ = cards;

    // Important: do NOT prune engine signals/actions based on the visible card slice.
    //
    // Rationale:
    // - Meaning-mode hides drafts by default for low-noise UX, but the reasoning engine must still
    //   surface “hidden-but-important” discipline signals (BM4/BM9, publish hygiene, etc.).
    // - Strict gates and resume HUDs rely on these signals/actions even when the underlying cards
    //   are not included in the current output slice due to visibility or budgeting.
    //
    // Keeping refs intact is intentional: callers can disclose/include_drafts or open by id.
    let _ = obj;
}
