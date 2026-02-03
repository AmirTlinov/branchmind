#![forbid(unsafe_code)]

//! Proof UX helpers.
//!
//! Goal: make “proof” capture copy/paste-ready for agents, while keeping daily outputs low-noise.

use serde_json::Value;

const PROOF_CHECK_CMD: &str = "CMD: <fill: command you ran>";
const PROOF_CHECK_LINK: &str = "LINK: <fill: CI run / artifact / log>";

#[derive(Clone, Debug, Default)]
pub(crate) struct ProofReceiptsLint {
    pub(crate) any_tagged: bool,
    pub(crate) cmd_receipt: bool,
    pub(crate) link_receipt: bool,
    pub(crate) cmd_placeholder: bool,
    pub(crate) link_placeholder: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProofParsePolicy {
    Warn,
    Strict,
}

impl ProofParsePolicy {
    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "warn" => Some(Self::Warn),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProofParseOutcome {
    pub(crate) checks: Vec<String>,
    pub(crate) attachments: Vec<String>,
    pub(crate) notes: Vec<String>,
}

pub(crate) fn proof_checks_placeholder_json() -> Value {
    Value::Array(vec![
        Value::String(PROOF_CHECK_CMD.to_string()),
        Value::String(PROOF_CHECK_LINK.to_string()),
    ])
}

pub(crate) fn proof_placeholder_json(checkpoint: Option<Value>) -> Value {
    let checkpoint = checkpoint.filter(|v| !v.is_null());

    // Default checkpoint for proof is "tests". Keep the portal call copy/paste-minimal:
    // when we only need tests proof, emit the checks[] directly (array form).
    //
    // For multi-checkpoint proof, emit the object form with explicit checkpoint.
    let tests_only = match checkpoint.as_ref() {
        None => true,
        Some(Value::String(s)) => s.as_str() == "tests",
        Some(Value::Array(arr)) => {
            !arr.is_empty() && arr.iter().all(|v| v.as_str().is_some_and(|s| s == "tests"))
        }
        _ => false,
    };
    if tests_only {
        return proof_checks_placeholder_json();
    }

    let mut obj = serde_json::Map::<String, Value>::new();
    if let Some(cp) = checkpoint {
        obj.insert("checkpoint".to_string(), cp);
    }
    obj.insert("checks".to_string(), proof_checks_placeholder_json());
    Value::Object(obj)
}

pub(crate) fn proof_checkpoint_value_for_missing(
    tests: bool,
    security: bool,
    perf: bool,
    docs: bool,
) -> Option<Value> {
    let mut out = Vec::new();
    if tests {
        out.push(Value::String("tests".to_string()));
    }
    if security {
        out.push(Value::String("security".to_string()));
    }
    if perf {
        out.push(Value::String("perf".to_string()));
    }
    if docs {
        out.push(Value::String("docs".to_string()));
    }
    match out.len() {
        0 => None,
        1 => out.into_iter().next(),
        _ => Some(Value::Array(out)),
    }
}

fn strip_markdown_prefixes(line: &str) -> &str {
    // Agents often paste proofs as markdown lists/quotes. Keep parsing deterministic and forgiving:
    // strip common bullet prefixes without touching valid CLI flags like "-Z" (no space).
    let mut s = line.trim_start();

    // Blockquote marker.
    if let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }

    for prefix in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest;
        }
    }

    // Numbered lists: "1. " or "1) ".
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0
        && i + 1 < bytes.len()
        && (bytes[i] == b'.' || bytes[i] == b')')
        && bytes[i + 1] == b' '
    {
        return &s[(i + 2)..];
    }

    s
}

fn strip_wrapping_angle_brackets(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.len() >= 2
        && trimmed.as_bytes()[0] == b'<'
        && trimmed.as_bytes()[trimmed.len() - 1] == b'>'
    {
        return trimmed[1..trimmed.len() - 1].trim();
    }
    trimmed
}

pub(crate) fn looks_like_bare_url(raw: &str) -> bool {
    let trimmed = strip_wrapping_angle_brackets(raw);
    trimmed
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        || trimmed
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
        || trimmed
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("file://"))
}

fn parse_receipt_line(line: &str, prefix: &str) -> Option<(bool, bool)> {
    let trimmed = strip_markdown_prefixes(line).trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() < prefix.len() {
        return None;
    }
    if !trimmed
        .get(..prefix.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    {
        return None;
    }

    // Receipts are expected to carry content after the prefix.
    let rest = trimmed[prefix.len()..].trim();
    let is_placeholder = rest.is_empty() || rest.contains("<fill");
    let is_receipt = !is_placeholder;
    Some((is_receipt, is_placeholder))
}

pub(crate) fn coerce_proof_check_line(raw: &str) -> Option<String> {
    let trimmed = strip_markdown_prefixes(raw).trim();
    if trimmed.is_empty() {
        return None;
    }

    let cmd_tagged = trimmed
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
        || (trimmed
            .get(..3)
            .is_some_and(|p| p.eq_ignore_ascii_case("CMD"))
            && trimmed
                .as_bytes()
                .get(3)
                .is_some_and(|b| b.is_ascii_whitespace()));
    if cmd_tagged {
        let rest = if trimmed
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
        {
            trimmed.get(4..).unwrap_or_default()
        } else {
            trimmed.get(3..).unwrap_or_default()
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Some("CMD:".to_string());
        }
        return Some(format!("CMD: {rest}"));
    }

    let link_tagged = trimmed
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
        || (trimmed
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("LINK"))
            && trimmed
                .as_bytes()
                .get(4)
                .is_some_and(|b| b.is_ascii_whitespace()));
    if link_tagged {
        let rest = if trimmed
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
        {
            trimmed.get(5..).unwrap_or_default()
        } else {
            trimmed.get(4..).unwrap_or_default()
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Some("LINK:".to_string());
        }
        return Some(format!("LINK: {rest}"));
    }

    // Heuristic: if a line is a bare URL, treat it as a LINK receipt; otherwise treat it as a CMD.
    // Keep the rule intentionally simple and deterministic.
    let url_candidate = strip_wrapping_angle_brackets(trimmed);
    if looks_like_bare_url(url_candidate) {
        Some(format!("LINK: {url_candidate}"))
    } else {
        Some(format!("CMD: {trimmed}"))
    }
}

pub(crate) fn coerce_proof_input_line(raw: &str) -> Option<(String, bool)> {
    let trimmed = strip_markdown_prefixes(raw).trim();
    if trimmed.is_empty() {
        return None;
    }

    // Agents commonly paste shell prompts. Keep the UX forgiving without accepting random prose.
    let trimmed = if let Some(rest) = trimmed.strip_prefix("$ ") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix("> ") {
        rest.trim()
    } else {
        trimmed
    };

    let cmd_tagged = trimmed
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
        || (trimmed
            .get(..3)
            .is_some_and(|p| p.eq_ignore_ascii_case("CMD"))
            && trimmed
                .as_bytes()
                .get(3)
                .is_some_and(|b| b.is_ascii_whitespace()));
    if cmd_tagged {
        let rest = if trimmed
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
        {
            trimmed.get(4..).unwrap_or_default()
        } else {
            trimmed.get(3..).unwrap_or_default()
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(("CMD:".to_string(), false));
        }
        return Some((format!("CMD: {rest}"), false));
    }

    let link_tagged = trimmed
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
        || (trimmed
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("LINK"))
            && trimmed
                .as_bytes()
                .get(4)
                .is_some_and(|b| b.is_ascii_whitespace()));
    if link_tagged {
        let rest = if trimmed
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
        {
            trimmed.get(5..).unwrap_or_default()
        } else {
            trimmed.get(4..).unwrap_or_default()
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(("LINK:".to_string(), false));
        }
        return Some((format!("LINK: {rest}"), false));
    }

    let url_candidate = strip_wrapping_angle_brackets(trimmed);
    if looks_like_bare_url(url_candidate) {
        return Some((format!("LINK: {url_candidate}"), false));
    }

    if looks_like_shell_command_line(trimmed) {
        return Some((format!("CMD: {trimmed}"), false));
    }

    if looks_like_path_line(trimmed) {
        let candidate = strip_wrapping_angle_brackets(trimmed).trim();
        return Some((format!("FILE: {candidate}"), false));
    }

    let note = strip_wrapping_angle_brackets(trimmed).trim();
    Some((format!("NOTE: {note}"), true))
}

pub(crate) fn parse_proof_input_lines(raw: &[String]) -> ProofParseOutcome {
    let mut outcome = ProofParseOutcome::default();
    for item in raw {
        for line in item.lines() {
            if let Some((coerced, ambiguous)) = coerce_proof_input_line(line) {
                if ambiguous {
                    outcome.notes.push(coerced);
                    continue;
                }
                if coerced.starts_with("FILE:") {
                    outcome.attachments.push(coerced);
                } else {
                    outcome.checks.push(coerced);
                }
            }
        }
    }
    outcome.checks = normalize_proof_checks(&outcome.checks);
    outcome
}

pub(crate) fn lint_proof_checks(checks: &[String]) -> ProofReceiptsLint {
    let mut out = ProofReceiptsLint::default();

    for raw in checks {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((is_receipt, is_placeholder)) = parse_receipt_line(line, "CMD:") {
            out.any_tagged = true;
            out.cmd_receipt |= is_receipt;
            out.cmd_placeholder |= is_placeholder;
            continue;
        }
        if let Some((is_receipt, is_placeholder)) = parse_receipt_line(line, "LINK:") {
            out.any_tagged = true;
            out.link_receipt |= is_receipt;
            out.link_placeholder |= is_placeholder;
            continue;
        }
    }

    out
}

pub(crate) fn normalize_proof_checks(checks: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in checks {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Never treat placeholders as proof receipts.
        if trimmed.contains("<fill") {
            continue;
        }
        if let Some((_is_receipt, is_placeholder)) = parse_receipt_line(trimmed, "CMD:")
            && is_placeholder
        {
            continue;
        }
        if let Some((_is_receipt, is_placeholder)) = parse_receipt_line(trimmed, "LINK:")
            && is_placeholder
        {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub(crate) fn extract_proof_checks_from_text(raw: &str) -> Vec<String> {
    // Conservative salvage: extract only obvious receipt-like lines from arbitrary text
    // (notes, runner reports, etc) to avoid turning normal prose into proof.
    //
    // Accepted candidates:
    // - tagged receipts: CMD:/LINK: (or "CMD <...>", "LINK <...>")
    // - bare URLs (http/https/file)
    let mut checks = Vec::<String>::new();
    for line in raw.lines() {
        let trimmed = strip_markdown_prefixes(line).trim();
        if trimmed.is_empty() {
            continue;
        }

        let cmd_tagged = trimmed
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
            || (trimmed
                .get(..3)
                .is_some_and(|p| p.eq_ignore_ascii_case("CMD"))
                && trimmed
                    .as_bytes()
                    .get(3)
                    .is_some_and(|b| b.is_ascii_whitespace()));
        let link_tagged = trimmed
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
            || (trimmed
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case("LINK"))
                && trimmed
                    .as_bytes()
                    .get(4)
                    .is_some_and(|b| b.is_ascii_whitespace()));

        let url_candidate = strip_wrapping_angle_brackets(trimmed);
        let url_tagged = looks_like_bare_url(url_candidate);

        if !(cmd_tagged || link_tagged || url_tagged) {
            continue;
        }

        if let Some(coerced) = coerce_proof_check_line(line) {
            checks.push(coerced);
        }
    }

    normalize_proof_checks(&checks)
}

fn looks_like_shell_command_line(trimmed: &str) -> bool {
    // Conservative heuristic: accept only strong shell-command-looking lines.
    // This is used for "salvage" paths to reduce false proof-gate loops when
    // agents put proof in free-text instead of refs/checks.
    let s = trimmed.trim();
    if s.is_empty() {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    let strong_prefixes = [
        "cargo ",
        "pytest ",
        "go test",
        "npm ",
        "pnpm ",
        "yarn ",
        "bun ",
        "make ",
        "just ",
        "git ",
        "rg ",
        "python ",
        "python3 ",
        "node ",
        "deno ",
        "docker ",
        "kubectl ",
        "helm ",
        "terraform ",
    ];
    strong_prefixes
        .into_iter()
        .any(|p| lower == p.trim_end() || lower.starts_with(p))
}

fn looks_like_path_line(trimmed: &str) -> bool {
    // Deterministic, conservative path heuristic (unix-first):
    // - absolute paths: /tmp/foo
    // - relative paths: ./foo, ../foo
    // - home paths: ~/foo
    //
    // We intentionally do NOT treat "file://..." as a path here; that is a URL and handled
    // earlier as a LINK receipt.
    let s = strip_wrapping_angle_brackets(trimmed).trim();
    if s.is_empty() {
        return false;
    }
    if looks_like_bare_url(s) {
        return false;
    }
    s.starts_with('/') || s.starts_with("./") || s.starts_with("../") || s.starts_with("~/")
}

fn push_unique_bounded(
    out: &mut Vec<String>,
    seen: &mut std::collections::BTreeSet<String>,
    v: String,
) {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return;
    }
    // Keep job event refs within storage bounds (defensive; storage also enforces).
    // 128 chars matches MAX_EVENT_REFS_ITEM_LEN in storage.
    let bounded: String = trimmed.chars().take(128).collect();
    if bounded.trim().is_empty() {
        return;
    }
    if seen.insert(bounded.clone()) {
        out.push(bounded);
    }
}

fn salvage_refs_from_text(text: &str) -> Vec<String> {
    // Extract stable references from arbitrary text (notes, summaries).
    // Goal: make "proof in text" usable without turning prose into fake proof.
    //
    // We salvage:
    // - receipts: CMD:/LINK: lines + bare URLs (via extract_proof_checks_from_text)
    // - strong shell-like bullet/inline commands (prefix allowlist)
    // - embedded stable ids: CARD-/TASK-/PLAN-/JOB-/notes@ and anchors a:*
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    for r in extract_proof_checks_from_text(text) {
        push_unique_bounded(&mut out, &mut seen, r);
        if out.len() >= 32 {
            return out;
        }
    }

    for line in text.lines() {
        let trimmed = strip_markdown_prefixes(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        // Agents often paste proof as "- cargo test -q" or "$ cargo test -q".
        let mut candidate = trimmed;
        if let Some(rest) = candidate.strip_prefix("$ ") {
            candidate = rest.trim();
        } else if let Some(rest) = candidate.strip_prefix("> ") {
            candidate = rest.trim();
        }
        if !candidate.is_empty() && looks_like_shell_command_line(candidate) {
            push_unique_bounded(&mut out, &mut seen, format!("CMD: {candidate}"));
            if out.len() >= 32 {
                return out;
            }
        }
    }

    // Tokenize on common separators; keep it deterministic and cheap.
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
            )
    }) {
        let token =
            raw.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '`'));
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();

        if looks_like_bare_url(token) {
            push_unique_bounded(&mut out, &mut seen, format!("LINK: {token}"));
        } else if lower.starts_with("card-")
            || lower.starts_with("task-")
            || lower.starts_with("plan-")
            || lower.starts_with("job-")
            || lower.starts_with("notes@")
            || lower.starts_with("a:")
        {
            push_unique_bounded(&mut out, &mut seen, token.to_string());
        }

        if out.len() >= 32 {
            return out;
        }
    }

    out
}

pub(crate) fn salvage_job_completion_refs(
    summary: &str,
    job_id: &str,
    explicit_refs: &[String],
) -> Vec<String> {
    // Proof-first DX: merge explicit refs with any salvageable refs found in free-form text.
    // We never remove explicit refs; we only add stable refs that reduce needless proof-gate loops.
    //
    // Determinism: preserve explicit order first, then append salvaged refs in deterministic order.
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    for r in explicit_refs.iter() {
        push_unique_bounded(&mut out, &mut seen, r.clone());
        if out.len() >= 32 {
            return out;
        }
    }

    if !summary.trim().is_empty() {
        for r in salvage_refs_from_text(summary) {
            push_unique_bounded(&mut out, &mut seen, r);
            if out.len() >= 32 {
                return out;
            }
        }
    }

    // Keep the thread navigable even when proof is missing.
    if out.len() < 32 && !out.iter().any(|r| r == job_id) {
        out.push(job_id.to_string());
    }
    out
}
