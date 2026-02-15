#![forbid(unsafe_code)]

use serde_json::Value;
use std::collections::HashSet;

const SCOUT_MIN_ANCHORS: usize = 3;
const SCOUT_MIN_CHANGE_HINTS: usize = 2;
const SCOUT_MIN_TEST_HINTS: usize = 3;
const SCOUT_MIN_RISK_MAP: usize = 3;
const SCOUT_MIN_SUMMARY_CHARS: usize = 320;
const SCOUT_MIN_ANCHOR_UNIQUENESS: f64 = 0.80;
const SCOUT_MAX_REF_REDUNDANCY: f64 = 0.25;
const SCOUT_MAX_ANCHOR_OVERLAP: f64 = 0.35;

pub(crate) fn has_non_job_ref(job_id: &str, refs: &[String]) -> bool {
    refs.iter().any(|raw| {
        let r = raw.trim();
        if r.is_empty() {
            return false;
        }
        if r == job_id {
            return false;
        }
        // `JOB-*` (including `JOB-*@seq`) is navigation, not proof.
        if r.starts_with("JOB-") {
            return false;
        }
        // Anchors are meaning pointers; they do not prove completion.
        if r.starts_with("a:") {
            return false;
        }
        true
    })
}

fn has_strict_proof_ref(refs: &[String]) -> bool {
    refs.iter().any(|raw| {
        let r = raw.trim_start();
        r.starts_with("LINK:") || r.starts_with("CMD:") || r.starts_with("FILE:")
    })
}

pub(crate) fn has_done_proof_ref(job_id: &str, priority: &str, refs: &[String]) -> bool {
    if priority.trim().eq_ignore_ascii_case("HIGH") {
        has_strict_proof_ref(refs)
    } else {
        has_non_job_ref(job_id, refs)
    }
}

fn json_has_forbidden_keys(value: &Value) -> Option<String> {
    const FORBIDDEN: &[&str] = &["diff", "patch", "code", "apply", "unified_diff"];
    match value {
        Value::Object(obj) => {
            for key in obj.keys() {
                if FORBIDDEN.iter().any(|f| key.eq_ignore_ascii_case(f)) {
                    return Some(key.clone());
                }
            }
            obj.values().find_map(json_has_forbidden_keys)
        }
        Value::Array(arr) => arr.iter().find_map(json_has_forbidden_keys),
        _ => None,
    }
}

fn markdown_code_block_too_long(text: &str, max_lines: usize) -> bool {
    let mut in_block = false;
    let mut lines = 0usize;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_block && lines > max_lines {
                return true;
            }
            in_block = !in_block;
            if in_block {
                lines = 0;
            }
            continue;
        }
        if in_block {
            lines += 1;
        }
    }
    in_block && lines > max_lines
}

fn normalize_signature(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_code_ref_token(raw: &str) -> bool {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("code:") else {
        return false;
    };
    let Some((path_raw, rest)) = rest.split_once("#L") else {
        return false;
    };
    if path_raw.trim().is_empty() {
        return false;
    }
    let Some((start_raw, rest)) = rest.split_once("-L") else {
        return false;
    };
    let (end_raw, sha_raw) = match rest.split_once("@sha256:") {
        Some((end, sha)) => (end, Some(sha)),
        None => (rest, None),
    };
    let Ok(start_line) = start_raw.trim().parse::<u32>() else {
        return false;
    };
    let Ok(end_line) = end_raw.trim().parse::<u32>() else {
        return false;
    };
    if start_line == 0 || end_line == 0 || end_line < start_line {
        return false;
    }
    if let Some(sha_raw) = sha_raw {
        let sha = sha_raw.trim();
        if sha.len() != 64 {
            return false;
        }
        return sha.chars().all(|ch| ch.is_ascii_hexdigit());
    }
    true
}

fn code_ref_path_key(raw: &str) -> Option<String> {
    raw.strip_prefix("code:")
        .and_then(|rest| rest.split_once("#L").map(|(path, _)| path))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| path.to_ascii_lowercase())
}

fn change_hint_path_is_covered(path_key: &str, covered_paths: &HashSet<String>) -> bool {
    if path_key.is_empty() {
        return false;
    }
    if covered_paths.contains(path_key) {
        return true;
    }
    let directory = path_key.trim_end_matches('/');
    if directory.is_empty() || directory == "." {
        return false;
    }
    let prefix = format!("{directory}/");
    covered_paths
        .iter()
        .any(|covered| covered.starts_with(&prefix))
}

pub(crate) fn validate_pipeline_summary_contract(role: &str, summary: &str) -> Result<(), String> {
    let parsed: Value = serde_json::from_str(summary)
        .map_err(|_| format!("{role}: summary must be JSON object text"))?;
    let Some(obj) = parsed.as_object() else {
        return Err(format!("{role}: summary must be JSON object text"));
    };
    if role.eq_ignore_ascii_case("scout") {
        // v2 format: has "format_version": 2 and typed anchors with content.
        // v2 validation is relaxed on the runner side (MCP server does strict store-based checks).
        let format_version = obj
            .get("format_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if format_version >= 2 {
            return validate_scout_v2_runner(obj);
        }

        if let Some(key) = json_has_forbidden_keys(&parsed) {
            return Err(format!("scout_context_pack contains forbidden key `{key}`"));
        }
        if markdown_code_block_too_long(summary, 20) {
            return Err(
                "scout_context_pack contains markdown code block over 20 lines".to_string(),
            );
        }
        let code_refs = obj
            .get("code_refs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "scout_context_pack.code_refs must be array".to_string())?;
        if code_refs.len() < 3 {
            return Err("scout_context_pack.code_refs must have at least 3 entries".to_string());
        }
        for (idx, value) in code_refs.iter().enumerate() {
            let raw = value.as_str().unwrap_or_default();
            if !is_code_ref_token(raw) {
                return Err(format!(
                    "scout_context_pack.code_refs[{idx}] must be CODE_REF token (code:...#Lx-Ly[@sha256:...])"
                ));
            }
        }
        let unique_refs = code_refs
            .iter()
            .filter_map(|v| v.as_str().map(str::trim))
            .filter(|v| !v.is_empty())
            .collect::<HashSet<_>>()
            .len();
        let ref_redundancy = if code_refs.is_empty() {
            0.0
        } else {
            1.0 - (unique_refs as f64 / code_refs.len() as f64)
        };
        if ref_redundancy > SCOUT_MAX_REF_REDUNDANCY {
            return Err(format!(
                "scout_context_pack code_refs redundancy too high (ratio={ref_redundancy:.2}, allowed<={SCOUT_MAX_REF_REDUNDANCY:.2})"
            ));
        }
        let anchors = obj
            .get("anchors")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "scout_context_pack.anchors must be array".to_string())?;
        if anchors.len() < SCOUT_MIN_ANCHORS {
            return Err(format!(
                "scout_context_pack.anchors must have at least {SCOUT_MIN_ANCHORS} entries"
            ));
        }
        let mut signatures = Vec::<String>::new();
        for item in anchors {
            let Some(anchor_obj) = item.as_object() else {
                return Err("scout_context_pack.anchors[] must be object".to_string());
            };
            let id = anchor_obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or_default();
            let rationale = anchor_obj
                .get("rationale")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or_default();
            if id.is_empty() || rationale.is_empty() {
                return Err("scout_context_pack.anchors[] must include id + rationale".to_string());
            }
            signatures.push(normalize_signature(&format!("{id} {rationale}")));
        }
        let unique_signatures = signatures
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .collect::<HashSet<_>>()
            .len();
        let anchor_uniqueness = if signatures.is_empty() {
            0.0
        } else {
            unique_signatures as f64 / signatures.len() as f64
        };
        if anchor_uniqueness < SCOUT_MIN_ANCHOR_UNIQUENESS {
            return Err(format!(
                "scout_context_pack anchor uniqueness too low (ratio={anchor_uniqueness:.2}, required>={SCOUT_MIN_ANCHOR_UNIQUENESS:.2})"
            ));
        }
        let anchor_overlap = 1.0 - anchor_uniqueness;
        if anchor_overlap > SCOUT_MAX_ANCHOR_OVERLAP {
            return Err(format!(
                "scout_context_pack anchor overlap too high (ratio={anchor_overlap:.2}, allowed<={SCOUT_MAX_ANCHOR_OVERLAP:.2})"
            ));
        }
        let change_hints = obj
            .get("change_hints")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "scout_context_pack.change_hints must be array".to_string())?;
        if change_hints.len() < SCOUT_MIN_CHANGE_HINTS {
            return Err(format!(
                "scout_context_pack.change_hints must have at least {SCOUT_MIN_CHANGE_HINTS} entries"
            ));
        }
        let test_hints = obj
            .get("test_hints")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "scout_context_pack.test_hints must be array".to_string())?;
        if test_hints.len() < SCOUT_MIN_TEST_HINTS {
            return Err(format!(
                "scout_context_pack.test_hints must have at least {SCOUT_MIN_TEST_HINTS} entries"
            ));
        }
        let risk_map = obj
            .get("risk_map")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "scout_context_pack.risk_map must be array".to_string())?;
        if risk_map.len() < SCOUT_MIN_RISK_MAP {
            return Err(format!(
                "scout_context_pack.risk_map must have at least {SCOUT_MIN_RISK_MAP} entries"
            ));
        }
        let summary_for_builder = obj
            .get("summary_for_builder")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if summary_for_builder.is_empty() {
            return Err("scout_context_pack.summary_for_builder is required".to_string());
        }
        if summary_for_builder.chars().count() < SCOUT_MIN_SUMMARY_CHARS {
            return Err(format!(
                "scout_context_pack.summary_for_builder must be >= {SCOUT_MIN_SUMMARY_CHARS} chars"
            ));
        }
    } else if role.eq_ignore_ascii_case("builder") {
        if obj
            .get("slice_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_none()
        {
            return Err("builder_diff_batch.slice_id is required".to_string());
        }
        let changes = obj
            .get("changes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "builder_diff_batch.changes must be array".to_string())?;
        let has_context_request = if let Some(context_request) = obj.get("context_request") {
            let req = context_request
                .as_object()
                .ok_or_else(|| "builder_diff_batch.context_request must be object".to_string())?;
            if req
                .get("reason")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err("builder_diff_batch.context_request.reason is required".to_string());
            }
            let missing_context = req
                .get("missing_context")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    "builder_diff_batch.context_request.missing_context must be array".to_string()
                })?;
            if missing_context.is_empty() {
                return Err(
                    "builder_diff_batch.context_request.missing_context must not be empty"
                        .to_string(),
                );
            }
            for (idx, item) in missing_context.iter().enumerate() {
                if item
                    .as_str()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .is_none()
                {
                    return Err(format!(
                        "builder_diff_batch.context_request.missing_context[{idx}] must be non-empty string"
                    ));
                }
            }
            for field in ["suggested_scout_focus", "suggested_tests"] {
                let values = req.get(field).and_then(|v| v.as_array()).ok_or_else(|| {
                    format!("builder_diff_batch.context_request.{field} must be array")
                })?;
                for (idx, item) in values.iter().enumerate() {
                    if item
                        .as_str()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .is_none()
                    {
                        return Err(format!(
                            "builder_diff_batch.context_request.{field}[{idx}] must be non-empty string"
                        ));
                    }
                }
            }
            true
        } else {
            false
        };
        if changes.is_empty() && !has_context_request {
            return Err("builder_diff_batch.changes must not be empty".to_string());
        }
        if !changes.is_empty() && has_context_request {
            return Err(
                "builder_diff_batch.context_request requires changes=[] for context-only rework"
                    .to_string(),
            );
        }
        for (idx, change) in changes.iter().enumerate() {
            let Some(change_obj) = change.as_object() else {
                return Err(format!("builder_diff_batch.changes[{idx}] must be object"));
            };
            if change_obj
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(format!(
                    "builder_diff_batch.changes[{idx}].path is required"
                ));
            }
            if change_obj
                .get("intent")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(format!(
                    "builder_diff_batch.changes[{idx}].intent is required"
                ));
            }
            if change_obj
                .get("diff_ref")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(format!(
                    "builder_diff_batch.changes[{idx}].diff_ref is required"
                ));
            }
        }
        let checks_to_run = obj
            .get("checks_to_run")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "builder_diff_batch.checks_to_run must be array".to_string())?;
        if checks_to_run.is_empty() && !has_context_request {
            return Err("builder_diff_batch.checks_to_run must not be empty".to_string());
        }
        let proof_refs = obj
            .get("proof_refs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "builder_diff_batch.proof_refs must be array".to_string())?;
        if proof_refs.is_empty() {
            return Err("builder_diff_batch.proof_refs must not be empty".to_string());
        }
        for (idx, item) in proof_refs.iter().enumerate() {
            let Some(raw) = item.as_str() else {
                return Err(format!(
                    "builder_diff_batch.proof_refs[{idx}] must be string"
                ));
            };
            let raw = raw.trim_start();
            if !(raw.starts_with("CMD:") || raw.starts_with("LINK:") || raw.starts_with("FILE:")) {
                return Err(format!(
                    "builder_diff_batch.proof_refs[{idx}] must use CMD:/LINK:/FILE:"
                ));
            }
        }
        if obj
            .get("rollback_plan")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_none()
        {
            return Err("builder_diff_batch.rollback_plan is required".to_string());
        }
        let evidence = obj
            .get("execution_evidence")
            .and_then(|v| v.as_object())
            .ok_or_else(|| "builder_diff_batch.execution_evidence is required".to_string())?;
        let revision = evidence
            .get("revision")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if revision <= 0 {
            return Err("builder_diff_batch.execution_evidence.revision must be > 0".to_string());
        }
        if evidence
            .get("diff_scope")
            .and_then(|v| v.as_array())
            .is_none_or(|v| v.is_empty())
            && !has_context_request
        {
            return Err(
                "builder_diff_batch.execution_evidence.diff_scope must not be empty".to_string(),
            );
        }
        let command_runs = evidence
            .get("command_runs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                "builder_diff_batch.execution_evidence.command_runs must be array".to_string()
            })?;
        if command_runs.is_empty() {
            return Err(
                "builder_diff_batch.execution_evidence.command_runs must not be empty".to_string(),
            );
        }
        for (idx, item) in command_runs.iter().enumerate() {
            let Some(run) = item.as_object() else {
                return Err(format!(
                    "builder_diff_batch.execution_evidence.command_runs[{idx}] must be object"
                ));
            };
            if run
                .get("cmd")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(format!(
                    "builder_diff_batch.execution_evidence.command_runs[{idx}].cmd is required"
                ));
            }
            if run.get("exit_code").and_then(|v| v.as_i64()).is_none() {
                return Err(format!(
                    "builder_diff_batch.execution_evidence.command_runs[{idx}].exit_code is required"
                ));
            }
            for field in ["stdout_ref", "stderr_ref"] {
                if run
                    .get(field)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .is_none()
                {
                    return Err(format!(
                        "builder_diff_batch.execution_evidence.command_runs[{idx}].{field} is required"
                    ));
                }
            }
        }
        let rollback_proof = evidence
            .get("rollback_proof")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                "builder_diff_batch.execution_evidence.rollback_proof is required".to_string()
            })?;
        if rollback_proof
            .get("strategy")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_none()
        {
            return Err(
                "builder_diff_batch.execution_evidence.rollback_proof.strategy is required"
                    .to_string(),
            );
        }
        if rollback_proof
            .get("target_revision")
            .and_then(|v| v.as_i64())
            .is_none()
        {
            return Err(
                "builder_diff_batch.execution_evidence.rollback_proof.target_revision is required"
                    .to_string(),
            );
        }
        let verification_cmd_ref = rollback_proof
            .get("verification_cmd_ref")
            .and_then(|v| v.as_str())
            .map(str::trim_start)
            .ok_or_else(|| {
                "builder_diff_batch.execution_evidence.rollback_proof.verification_cmd_ref is required"
                    .to_string()
            })?;
        if !(verification_cmd_ref.starts_with("CMD:")
            || verification_cmd_ref.starts_with("LINK:")
            || verification_cmd_ref.starts_with("FILE:"))
        {
            return Err(
                "builder_diff_batch.execution_evidence.rollback_proof.verification_cmd_ref must use CMD:/LINK:/FILE:"
                    .to_string(),
            );
        }
        let semantic_guards = evidence
            .get("semantic_guards")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                "builder_diff_batch.execution_evidence.semantic_guards is required".to_string()
            })?;
        for field in ["must_should_may_delta", "contract_term_consistency"] {
            if semantic_guards
                .get(field)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(format!(
                    "builder_diff_batch.execution_evidence.semantic_guards.{field} is required"
                ));
            }
        }
        if evidence
            .get("diff_scope")
            .and_then(|v| v.as_array())
            .is_some_and(|scope| {
                scope
                    .iter()
                    .any(|item| item.as_str().map(str::trim).is_none_or(|v| v.is_empty()))
            })
        {
            return Err(
                "builder_diff_batch.execution_evidence.diff_scope[] must be non-empty strings"
                    .to_string(),
            );
        }
    } else if role.eq_ignore_ascii_case("validator") {
        let recommendation = obj
            .get("recommendation")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // v2 adds writer_retry, scout_retry, escalate.
        let valid_recs = [
            "approve",
            "rework",
            "reject",
            "writer_retry",
            "scout_retry",
            "escalate",
        ];
        if !valid_recs.contains(&recommendation.as_str()) {
            return Err(format!(
                "validator_report.recommendation must be one of: {}",
                valid_recs.join("|")
            ));
        }
        // v2: intent_compliance present → skip plan_fit_score check.
        let is_v2 = obj.get("intent_compliance").is_some();
        if !is_v2 {
            let plan_fit = obj
                .get("plan_fit_score")
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            if !(0..=100).contains(&plan_fit) {
                return Err("validator_report.plan_fit_score must be 0..100".to_string());
            }
        }
    } else if role.eq_ignore_ascii_case("writer") {
        validate_writer_runner(obj)?;
    }
    Ok(())
}

/// Runner-side v2 scout validation.
/// Lighter than MCP-side (no store access), checks structural shape.
fn validate_scout_v2_runner(obj: &serde_json::Map<String, Value>) -> Result<(), String> {
    let parsed_root = Value::Object(obj.clone());
    if let Some(key) = json_has_forbidden_keys(&parsed_root) {
        return Err(format!(
            "scout_context_pack_v2 contains forbidden key `{key}`"
        ));
    }

    if obj
        .get("objective")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err("scout_context_pack_v2.objective is required".to_string());
    }

    let anchors = obj
        .get("anchors")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "scout_context_pack_v2.anchors must be array".to_string())?;
    if anchors.len() < SCOUT_MIN_ANCHORS {
        return Err(format!(
            "scout_context_pack_v2.anchors must have at least {SCOUT_MIN_ANCHORS} entries"
        ));
    }

    let valid_types = ["primary", "dependency", "reference", "structural"];
    let mut primary_structural_paths = HashSet::<String>::new();
    let mut any_anchor_paths = HashSet::<String>::new();
    for (idx, item) in anchors.iter().enumerate() {
        let Some(anchor_obj) = item.as_object() else {
            return Err(format!(
                "scout_context_pack_v2.anchors[{idx}] must be object"
            ));
        };
        let id = anchor_obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(format!(
                "scout_context_pack_v2.anchors[{idx}].id is required"
            ));
        }
        let anchor_type = anchor_obj
            .get("anchor_type")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !valid_types.contains(&anchor_type.as_str()) {
            return Err(format!(
                "scout_context_pack_v2.anchors[{idx}].anchor_type must be primary|dependency|reference|structural"
            ));
        }
        let code_ref = anchor_obj
            .get("code_ref")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if code_ref.is_empty() || !code_ref.starts_with("code:") {
            return Err(format!(
                "scout_context_pack_v2.anchors[{idx}].code_ref must be code:... format"
            ));
        }
        if let Some(path_key) = code_ref_path_key(code_ref) {
            any_anchor_paths.insert(path_key.clone());
            if matches!(anchor_type.as_str(), "primary" | "structural") {
                primary_structural_paths.insert(path_key);
            }
        }
        // Content required for primary/dependency/reference.
        if matches!(anchor_type.as_str(), "primary" | "dependency" | "reference") {
            let content = anchor_obj
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if content.trim().is_empty() {
                return Err(format!(
                    "scout_context_pack_v2.anchors[{idx}].content is required for {anchor_type} type"
                ));
            }
        }
    }

    let change_hints = obj
        .get("change_hints")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "scout_context_pack_v2.change_hints must be array".to_string())?;
    if change_hints.len() < SCOUT_MIN_CHANGE_HINTS {
        return Err(format!(
            "scout_context_pack_v2.change_hints must have at least {SCOUT_MIN_CHANGE_HINTS} entries"
        ));
    }
    let covered_paths = if primary_structural_paths.is_empty() {
        any_anchor_paths
    } else {
        primary_structural_paths
    };
    let mut missing_paths = Vec::<String>::new();
    for (idx, hint) in change_hints.iter().enumerate() {
        let Some(hint_obj) = hint.as_object() else {
            return Err(format!(
                "scout_context_pack_v2.change_hints[{idx}] must be object"
            ));
        };
        let path = hint_obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if path.is_empty() {
            return Err(format!(
                "scout_context_pack_v2.change_hints[{idx}].path is required"
            ));
        }
        let path_key = path.to_ascii_lowercase();
        if !change_hint_path_is_covered(&path_key, &covered_paths) {
            missing_paths.push(path.to_string());
        }
    }
    if !missing_paths.is_empty() {
        return Err(format!(
            "scout_context_pack_v2 missing primary/structural anchor coverage for change_hints: {}",
            missing_paths.join(", ")
        ));
    }

    let summary = obj
        .get("summary_for_builder")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if summary.is_empty() {
        return Err("scout_context_pack_v2.summary_for_builder is required".to_string());
    }

    Ok(())
}

/// Runner-side writer validation.
/// Checks structural shape of writer_patch_pack without store access.
fn validate_writer_runner(obj: &serde_json::Map<String, Value>) -> Result<(), String> {
    if obj
        .get("slice_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err("writer_patch_pack.slice_id is required".to_string());
    }

    // insufficient_context is an escape hatch — if set, patches can be empty.
    let has_escape = obj
        .get("insufficient_context")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());

    let patches = obj
        .get("patches")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "writer_patch_pack.patches must be array".to_string())?;

    if patches.is_empty() && !has_escape {
        return Err(
            "writer_patch_pack.patches must not be empty (set insufficient_context to skip)"
                .to_string(),
        );
    }

    let valid_kinds = [
        "replace",
        "insert_after",
        "insert_before",
        "create_file",
        "delete_file",
    ];

    for (fi, file_patch) in patches.iter().enumerate() {
        let Some(fp_obj) = file_patch.as_object() else {
            return Err(format!("writer_patch_pack.patches[{fi}] must be object"));
        };
        let path = fp_obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if path.is_empty() {
            return Err(format!("writer_patch_pack.patches[{fi}].path is required"));
        }
        if path.contains("..") {
            return Err(format!(
                "writer_patch_pack.patches[{fi}].path contains path traversal"
            ));
        }
        let ops = fp_obj
            .get("ops")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("writer_patch_pack.patches[{fi}].ops must be array"))?;
        if ops.is_empty() {
            return Err(format!(
                "writer_patch_pack.patches[{fi}].ops must not be empty"
            ));
        }
        for (oi, op) in ops.iter().enumerate() {
            let Some(op_obj) = op.as_object() else {
                return Err(format!(
                    "writer_patch_pack.patches[{fi}].ops[{oi}] must be object"
                ));
            };
            let kind = op_obj
                .get("kind")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !valid_kinds.contains(&kind.as_str()) {
                return Err(format!(
                    "writer_patch_pack.patches[{fi}].ops[{oi}].kind must be one of: {}",
                    valid_kinds.join("|")
                ));
            }
            match kind.as_str() {
                "replace" => {
                    if op_obj
                        .get("old_lines")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].old_lines required for replace"
                        ));
                    }
                    if op_obj.get("new_lines").and_then(|v| v.as_array()).is_none() {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].new_lines required for replace"
                        ));
                    }
                }
                "insert_after" => {
                    if op_obj
                        .get("after")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].after required for insert_after"
                        ));
                    }
                    if op_obj
                        .get("content")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].content required for insert_after"
                        ));
                    }
                }
                "insert_before" => {
                    if op_obj
                        .get("before")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].before required for insert_before"
                        ));
                    }
                    if op_obj
                        .get("content")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].content required for insert_before"
                        ));
                    }
                }
                "create_file" => {
                    if op_obj
                        .get("content")
                        .and_then(|v| v.as_array())
                        .is_none_or(|v| v.is_empty())
                    {
                        return Err(format!(
                            "writer_patch_pack.patches[{fi}].ops[{oi}].content required for create_file"
                        ));
                    }
                }
                // delete_file needs no extra fields.
                _ => {}
            }
        }
    }

    if obj
        .get("summary")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err("writer_patch_pack.summary is required".to_string());
    }

    if obj
        .get("affected_files")
        .and_then(|v| v.as_array())
        .is_none()
    {
        return Err("writer_patch_pack.affected_files must be array".to_string());
    }

    Ok(())
}
