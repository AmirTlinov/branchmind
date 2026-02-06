#![forbid(unsafe_code)]

use crate::support::proof::looks_like_bare_url;
use serde_json::Value;

use super::text::{looks_like_placeholder, strip_markdown_list_prefix, strip_markdownish_prefixes};
use super::util::card_tags_lower;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct EvidenceReceipts {
    pub(super) cmd: bool,
    pub(super) link: bool,
    pub(super) cmd_placeholder: bool,
    pub(super) link_placeholder: bool,
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

pub(super) fn evidence_receipts(card: &Value) -> EvidenceReceipts {
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

pub(super) fn evidence_strength_score(
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

pub(super) struct ConfidenceContext<'a, 'v> {
    pub(super) by_id: &'a std::collections::BTreeMap<String, &'v Value>,
    pub(super) incoming_supports: &'a std::collections::BTreeMap<String, Vec<String>>,
    pub(super) incoming_blocks: &'a std::collections::BTreeMap<String, Vec<String>>,
    pub(super) evidence_scores: &'a std::collections::BTreeMap<String, u8>,
}

pub(super) fn confidence_for_id(
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

pub(super) fn extract_cmd_from_test_card(card: &Value) -> Option<String> {
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
