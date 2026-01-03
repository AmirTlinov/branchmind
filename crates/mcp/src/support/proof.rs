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

fn parse_receipt_line(line: &str, prefix: &str) -> Option<(bool, bool)> {
    let trimmed = line.trim();
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
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("CMD:"))
    {
        let rest = trimmed.get(4..).unwrap_or_default().trim();
        if rest.is_empty() {
            return Some("CMD:".to_string());
        }
        return Some(format!("CMD: {rest}"));
    }
    if trimmed
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("LINK:"))
    {
        let rest = trimmed.get(5..).unwrap_or_default().trim();
        if rest.is_empty() {
            return Some("LINK:".to_string());
        }
        return Some(format!("LINK: {rest}"));
    }

    // Heuristic: if a line is a bare URL, treat it as a LINK receipt; otherwise treat it as a CMD.
    // Keep the rule intentionally simple and deterministic.
    let is_bare_url = trimmed
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        || trimmed
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
        || trimmed
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("file://"));
    if is_bare_url {
        Some(format!("LINK: {trimmed}"))
    } else {
        Some(format!("CMD: {trimmed}"))
    }
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
