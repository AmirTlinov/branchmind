#![forbid(unsafe_code)]

//! Pipeline v2 contracts: writer validation, pre-validator, post-validator v2,
//! cascade state machine, and cross-validation.

use super::ratio_0_1;
use super::{
    ScoutAnchorType, ScoutAnchorV2, ValidatorReportContract, ai_error, require_object,
    require_string, string_array,
};
use serde_json::{Value, json};
use std::collections::HashSet;

// ── Writer Patch Pack Validation ──

const WRITER_MAX_PATCHES: usize = 50;
const WRITER_MAX_OPS_PER_FILE: usize = 30;
const WRITER_MAX_OLD_LINES: usize = 200;
const WRITER_VALID_KINDS: &[&str] = &[
    "replace",
    "insert_after",
    "insert_before",
    "create_file",
    "delete_file",
];

/// MCP-side strict validation of writer_patch_pack.
/// Checks structural correctness, path safety, and op field completeness.
pub(crate) fn validate_writer_patch_pack(raw: &Value) -> Result<Value, Value> {
    let obj = require_object(raw, "writer_patch_pack")?;
    let slice_id = require_string(obj, "slice_id", "writer_patch_pack")?;
    let summary = require_string(obj, "summary", "writer_patch_pack")?;
    let affected_files = string_array(obj, "affected_files", "writer_patch_pack")?;

    // insufficient_context is an escape hatch.
    let insufficient = obj
        .get("insufficient_context")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let patches_arr = obj
        .get("patches")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "writer_patch_pack.patches: expected array"))?;

    if patches_arr.is_empty() && insufficient.is_none() {
        return Err(ai_error(
            "INVALID_INPUT",
            "writer_patch_pack.patches must not be empty (set insufficient_context to skip)",
        ));
    }

    if patches_arr.len() > WRITER_MAX_PATCHES {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("writer_patch_pack.patches exceeds max {WRITER_MAX_PATCHES}"),
        ));
    }

    let mut normalized_patches = Vec::new();

    for (fi, file_patch) in patches_arr.iter().enumerate() {
        let fp = require_object(file_patch, &format!("writer_patch_pack.patches[{fi}]"))?;
        let path = require_string(fp, "path", &format!("writer_patch_pack.patches[{fi}]"))?;

        // Security: reject path traversal and absolute paths.
        if path.contains("..") || path.starts_with('/') {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("writer_patch_pack.patches[{fi}].path rejected: {path}"),
            ));
        }

        let ops_arr = fp.get("ops").and_then(|v| v.as_array()).ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("writer_patch_pack.patches[{fi}].ops: expected array"),
            )
        })?;

        if ops_arr.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("writer_patch_pack.patches[{fi}].ops must not be empty"),
            ));
        }

        if ops_arr.len() > WRITER_MAX_OPS_PER_FILE {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "writer_patch_pack.patches[{fi}].ops exceeds max {WRITER_MAX_OPS_PER_FILE}"
                ),
            ));
        }

        let mut normalized_ops = Vec::new();
        for (oi, op) in ops_arr.iter().enumerate() {
            let op_obj = require_object(op, &format!("writer_patch_pack.patches[{fi}].ops[{oi}]"))?;
            let kind = require_string(
                op_obj,
                "kind",
                &format!("writer_patch_pack.patches[{fi}].ops[{oi}]"),
            )?
            .to_ascii_lowercase();

            if !WRITER_VALID_KINDS.contains(&kind.as_str()) {
                return Err(ai_error(
                    "INVALID_INPUT",
                    &format!("writer_patch_pack.patches[{fi}].ops[{oi}].kind invalid: {kind}"),
                ));
            }

            let field = format!("writer_patch_pack.patches[{fi}].ops[{oi}]");
            let normalized_op = match kind.as_str() {
                "replace" => {
                    let old_lines = string_array(op_obj, "old_lines", &field)?;
                    if old_lines.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.old_lines must not be empty for replace"),
                        ));
                    }
                    if old_lines.len() > WRITER_MAX_OLD_LINES {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.old_lines exceeds max {WRITER_MAX_OLD_LINES} lines"),
                        ));
                    }
                    let new_lines = string_array(op_obj, "new_lines", &field)?;
                    let anchor_ref = op_obj
                        .get("anchor_ref")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    json!({
                        "kind": "replace",
                        "old_lines": old_lines,
                        "new_lines": new_lines,
                        "anchor_ref": anchor_ref
                    })
                }
                "insert_after" => {
                    let after = string_array(op_obj, "after", &field)?;
                    if after.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.after must not be empty for insert_after"),
                        ));
                    }
                    let content = string_array(op_obj, "content", &field)?;
                    if content.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.content must not be empty for insert_after"),
                        ));
                    }
                    json!({
                        "kind": "insert_after",
                        "after": after,
                        "content": content
                    })
                }
                "insert_before" => {
                    let before = string_array(op_obj, "before", &field)?;
                    if before.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.before must not be empty for insert_before"),
                        ));
                    }
                    let content = string_array(op_obj, "content", &field)?;
                    if content.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.content must not be empty for insert_before"),
                        ));
                    }
                    json!({
                        "kind": "insert_before",
                        "before": before,
                        "content": content
                    })
                }
                "create_file" => {
                    let content = string_array(op_obj, "content", &field)?;
                    if content.is_empty() {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.content must not be empty for create_file"),
                        ));
                    }
                    json!({
                        "kind": "create_file",
                        "content": content
                    })
                }
                "delete_file" => {
                    json!({ "kind": "delete_file" })
                }
                _ => unreachable!(),
            };
            normalized_ops.push(normalized_op);
        }
        normalized_patches.push(json!({
            "path": path,
            "ops": normalized_ops
        }));
    }

    let checks_to_run = obj
        .get("checks_to_run")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(json!({
        "slice_id": slice_id,
        "patches": normalized_patches,
        "summary": summary,
        "affected_files": affected_files,
        "checks_to_run": checks_to_run,
        "insufficient_context": insufficient
    }))
}

// ── Pre-Validator: deterministic gate before writer (no LLM) ──

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreValidatorVerdict {
    Pass,
    NeedMore { hints: Vec<String> },
    Reject { reason: String },
}

#[derive(Clone, Debug)]
pub(crate) struct PreValidatorChecks {
    pub completeness_ok: bool,
    pub completeness_missing: Vec<String>,
    pub dependencies_ok: bool,
    pub dependencies_missing: Vec<String>,
    pub patterns_ok: bool,
    pub patterns_missing: Vec<String>,
    pub intent_coverage_ok: bool,
    pub intent_coverage_missing: Vec<String>,
}

fn synthesize_anchor_type(index: usize, total: usize) -> ScoutAnchorType {
    if index == 0 {
        ScoutAnchorType::Primary
    } else if index == 1 {
        ScoutAnchorType::Structural
    } else if index + 1 == total {
        ScoutAnchorType::Reference
    } else {
        ScoutAnchorType::Dependency
    }
}

fn synthesize_anchors_from_legacy_pack(normalized_pack: &Value) -> Vec<ScoutAnchorV2> {
    let code_refs: Vec<String> = normalized_pack
        .get("code_refs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::trim))
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let anchors_arr = normalized_pack
        .get("anchors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let anchors_total = anchors_arr.len().max(code_refs.len());

    let mut out = Vec::<ScoutAnchorV2>::new();

    for (idx, item) in anchors_arr.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("a:scout-anchor-{}", idx + 1));
        let rationale = obj
            .get("rationale")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("synthetic rationale from anchor[{idx}]"));
        let code_ref = obj
            .get("code_ref")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| code_refs.get(idx).cloned())
            .or_else(|| code_refs.first().cloned())
            .unwrap_or_default();
        if code_ref.is_empty() {
            continue;
        }
        let anchor_type = obj
            .get("anchor_type")
            .and_then(|v| v.as_str())
            .and_then(|v| ScoutAnchorType::from_str(v).ok())
            .unwrap_or_else(|| synthesize_anchor_type(idx, anchors_total.max(1)));
        let content = obj
            .get("content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| rationale.clone());
        let line_count = obj
            .get("line_count")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or_else(|| content.lines().count().max(1) as u32);
        let meta_hint = obj
            .get("meta_hint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        out.push(ScoutAnchorV2 {
            id,
            anchor_type,
            rationale,
            code_ref,
            content,
            line_count,
            meta_hint,
        });
    }

    if out.is_empty() {
        for (idx, code_ref) in code_refs.iter().enumerate() {
            out.push(ScoutAnchorV2 {
                id: format!("a:scout-code-ref-{}", idx + 1),
                anchor_type: synthesize_anchor_type(idx, code_refs.len().max(1)),
                rationale: format!("synthetic anchor from code_refs[{idx}]"),
                code_ref: code_ref.clone(),
                content: format!("synthetic content from code_refs[{idx}]"),
                line_count: 1,
                meta_hint: Some("synthetic_anchor_from_code_refs".to_string()),
            });
        }
    }

    out
}

/// Deterministic pre-validation of a v2 scout pack.
/// Runs pure Rust checks — no LLM, instant, free.
///
/// `normalized_pack` is the JSON from `validate_scout_context_pack_v2`.
/// `anchors_v2` is the parsed anchor vec from the same call.
pub(crate) fn pre_validate_scout_pack(
    normalized_pack: &Value,
    anchors_v2: &[ScoutAnchorV2],
) -> (PreValidatorVerdict, PreValidatorChecks) {
    let anchors: Vec<ScoutAnchorV2> = if anchors_v2.is_empty() {
        synthesize_anchors_from_legacy_pack(normalized_pack)
    } else {
        anchors_v2.to_vec()
    };

    let mut checks = PreValidatorChecks {
        completeness_ok: true,
        completeness_missing: Vec::new(),
        dependencies_ok: true,
        dependencies_missing: Vec::new(),
        patterns_ok: true,
        patterns_missing: Vec::new(),
        intent_coverage_ok: true,
        intent_coverage_missing: Vec::new(),
    };

    // Keep anchor payload fields "live" in runtime builds: they are part of the
    // normalized scout contract and intentionally preserved for downstream checks.
    for anchor in &anchors {
        let _ = anchor.anchor_type.as_str();
        let _ = (
            &anchor.id,
            &anchor.rationale,
            &anchor.content,
            anchor.line_count,
            &anchor.meta_hint,
        );
    }

    // Reject if no anchors at all.
    if anchors.is_empty() {
        return (
            PreValidatorVerdict::Reject {
                reason: "scout pack contains zero anchors".into(),
            },
            checks,
        );
    }

    // 1. Completeness: each change_hints[].path covered by primary or structural anchor.
    let covered_paths: HashSet<String> = anchors
        .iter()
        .filter(|a| {
            matches!(
                a.anchor_type,
                ScoutAnchorType::Primary | ScoutAnchorType::Structural
            )
        })
        .filter_map(|a| {
            a.code_ref
                .strip_prefix("code:")
                .and_then(|rest| rest.split_once("#L").map(|(path, _)| path.to_string()))
        })
        .collect();

    if let Some(change_hints) = normalized_pack
        .get("change_hints")
        .and_then(|v| v.as_array())
    {
        for hint in change_hints {
            if let Some(path) = hint.get("path").and_then(|v| v.as_str())
                && !covered_paths.contains(path)
            {
                checks.completeness_ok = false;
                checks
                    .completeness_missing
                    .push(format!("no primary/structural anchor for: {path}"));
            }
        }
    }

    // 2. Dependencies: each primary anchor should have at least one dependency
    //    anchor from the same file or its imports.
    let primary_files: Vec<String> = anchors
        .iter()
        .filter(|a| matches!(a.anchor_type, ScoutAnchorType::Primary))
        .filter_map(|a| {
            a.code_ref
                .strip_prefix("code:")
                .and_then(|rest| rest.split_once("#L").map(|(path, _)| path.to_string()))
        })
        .collect();

    let dependency_files: HashSet<String> = anchors
        .iter()
        .filter(|a| matches!(a.anchor_type, ScoutAnchorType::Dependency))
        .filter_map(|a| {
            a.code_ref
                .strip_prefix("code:")
                .and_then(|rest| rest.split_once("#L").map(|(path, _)| path.to_string()))
        })
        .collect();

    // Check: at least one dependency anchor exists somewhere.
    if dependency_files.is_empty() && !primary_files.is_empty() {
        checks.dependencies_ok = false;
        checks
            .dependencies_missing
            .push("no dependency anchors in pack".into());
    }

    // 3. Patterns: at least one reference anchor in the pack.
    let has_reference = anchors
        .iter()
        .any(|a| matches!(a.anchor_type, ScoutAnchorType::Reference));
    if !has_reference {
        checks.patterns_ok = false;
        checks
            .patterns_missing
            .push("no reference anchor for code style/patterns".into());
    }

    // 4. Intent coverage: coverage_matrix.objective_items must not be empty
    //    and must intersect with objective text (basic word overlap check).
    if let Some(coverage) = normalized_pack
        .get("coverage_matrix")
        .and_then(|v| v.as_object())
    {
        let obj_items = coverage
            .get("objective_items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if obj_items.is_empty() {
            checks.intent_coverage_ok = false;
            checks
                .intent_coverage_missing
                .push("coverage_matrix.objective_items is empty".into());
        }
    } else {
        // v2 packs may not have coverage_matrix yet — soft check.
        let objective = normalized_pack
            .get("objective")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if objective.is_empty() {
            checks.intent_coverage_ok = false;
            checks
                .intent_coverage_missing
                .push("objective is empty".into());
        }
    }

    // Determine verdict.
    let critical_fail = !checks.completeness_ok
        && checks.completeness_missing.len()
            > normalized_pack
                .get("change_hints")
                .and_then(|v| v.as_array())
                .map(|a| a.len() / 2)
                .unwrap_or(1);

    if critical_fail {
        let reason = format!(
            "too many uncovered change_hints: {}",
            checks.completeness_missing.join("; ")
        );
        return (PreValidatorVerdict::Reject { reason }, checks);
    }

    // Verdict policy:
    // - Completeness/patterns/intent coverage are *blocking* (NeedMore).
    // - Dependencies are *non-blocking* by default: scouts may legitimately omit dependency anchors
    //   on small slices, and we prefer forward progress over ping‑pong. The detailed signal remains
    //   in `checks.dependencies_*` for UX/reporting.
    let mut hints = Vec::new();
    if !checks.completeness_ok {
        hints.extend(checks.completeness_missing.iter().cloned());
    }
    if !checks.patterns_ok {
        hints.extend(checks.patterns_missing.iter().cloned());
    }
    if !checks.intent_coverage_ok {
        hints.extend(checks.intent_coverage_missing.iter().cloned());
    }

    if hints.is_empty() {
        (PreValidatorVerdict::Pass, checks)
    } else {
        (PreValidatorVerdict::NeedMore { hints }, checks)
    }
}

// ── Validator Report v1 ──

pub(crate) fn validate_validator_report(raw: &Value) -> Result<ValidatorReportContract, Value> {
    let obj = require_object(raw, "validator_report")?;
    let slice_id = require_string(obj, "slice_id", "validator_report")?;
    let plan_fit_score = obj
        .get("plan_fit_score")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "validator_report.plan_fit_score is required",
            )
        })?;
    if !(0..=100).contains(&plan_fit_score) {
        return Err(ai_error(
            "INVALID_INPUT",
            "validator_report.plan_fit_score must be in range 0..100",
        ));
    }

    let policy_checks = obj
        .get("policy_checks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.policy_checks: expected array"))?
        .iter()
        .map(|item| {
            let o = require_object(item, "validator_report.policy_checks[]")?;
            Ok(json!({
                "name": require_string(o, "name", "validator_report.policy_checks[]")?,
                "pass": o.get("pass").and_then(|v| v.as_bool()).ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.policy_checks[].pass is required"))?,
                "reason": require_string(o, "reason", "validator_report.policy_checks[]")?
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;

    let tests = obj
        .get("tests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.tests: expected array"))?
        .iter()
        .map(|item| {
            let o = require_object(item, "validator_report.tests[]")?;
            Ok(json!({
                "name": require_string(o, "name", "validator_report.tests[]")?,
                "pass": o.get("pass").and_then(|v| v.as_bool()).ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.tests[].pass is required"))?,
                "evidence_ref": require_string(o, "evidence_ref", "validator_report.tests[]")?
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;

    let security_findings = obj
        .get("security_findings")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "validator_report.security_findings: expected array",
            )
        })?;

    let regression_risk =
        require_string(obj, "regression_risk", "validator_report")?.to_ascii_lowercase();
    if !matches!(regression_risk.as_str(), "low" | "medium" | "high") {
        return Err(ai_error(
            "INVALID_INPUT",
            "validator_report.regression_risk must be low|medium|high",
        ));
    }
    let recommendation =
        require_string(obj, "recommendation", "validator_report")?.to_ascii_lowercase();
    if !matches!(recommendation.as_str(), "approve" | "rework" | "reject") {
        return Err(ai_error(
            "INVALID_INPUT",
            "validator_report.recommendation must be approve|rework|reject",
        ));
    }
    let rework_actions = string_array(obj, "rework_actions", "validator_report")?;

    Ok(ValidatorReportContract {
        recommendation: recommendation.clone(),
        normalized: json!({
            "slice_id": slice_id,
            "plan_fit_score": plan_fit_score,
            "policy_checks": policy_checks,
            "tests": tests,
            "security_findings": security_findings,
            "regression_risk": regression_risk,
            "recommendation": recommendation,
            "rework_actions": rework_actions
        }),
    })
}

// ── Post-Validator v2: IntentComplianceCheck + Traceability ──

const VALID_SECURITY_SEVERITIES: &[&str] = &["critical", "high", "medium", "low", "info"];
const MIN_TRACEABILITY_RATIO: f64 = 0.7;

/// Enhanced validator report with intent compliance, traceability, and typed verdicts.
/// Falls back to v1 when `intent_compliance` is absent.
pub(crate) fn validate_validator_report_v2(raw: &Value) -> Result<ValidatorReportContract, Value> {
    let obj = require_object(raw, "validator_report")?;

    // Check if this is v2 (has intent_compliance) or v1.
    let has_intent = obj.get("intent_compliance").is_some();
    if !has_intent {
        // Fall back to v1 validation.
        return validate_validator_report(raw);
    }

    let slice_id = require_string(obj, "slice_id", "validator_report")?;

    // Intent compliance.
    let intent = require_object(
        obj.get("intent_compliance").ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "validator_report.intent_compliance is required",
            )
        })?,
        "validator_report.intent_compliance",
    )?;
    let aspects_requested = string_array(
        intent,
        "aspects_requested",
        "validator_report.intent_compliance",
    )?;
    let aspects_fulfilled = string_array(
        intent,
        "aspects_fulfilled",
        "validator_report.intent_compliance",
    )?;
    let aspects_missing = string_array(
        intent,
        "aspects_missing",
        "validator_report.intent_compliance",
    )?;
    let fulfillment_ratio = ratio_0_1(
        intent,
        "fulfillment_ratio",
        "validator_report.intent_compliance",
    )?;

    // Optional: aspects_unexpected.
    let aspects_unexpected = intent
        .get("aspects_unexpected")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Traceability.
    let trace_obj = require_object(
        obj.get("traceability").ok_or_else(|| {
            ai_error("INVALID_INPUT", "validator_report.traceability is required")
        })?,
        "validator_report.traceability",
    )?;
    let writer_refs = string_array(
        trace_obj,
        "writer_refs_to_scout_anchors",
        "validator_report.traceability",
    )?;
    let untraced = string_array(
        trace_obj,
        "untraced_changes",
        "validator_report.traceability",
    )?;
    let traceability_ratio = ratio_0_1(
        trace_obj,
        "traceability_ratio",
        "validator_report.traceability",
    )?;

    // Policy checks (same as v1).
    let policy_checks = obj
        .get("policy_checks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.policy_checks: expected array"))?
        .iter()
        .map(|item| {
            let o = require_object(item, "validator_report.policy_checks[]")?;
            Ok(json!({
                "name": require_string(o, "name", "validator_report.policy_checks[]")?,
                "pass": o.get("pass").and_then(|v| v.as_bool()).ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.policy_checks[].pass is required"))?,
                "reason": require_string(o, "reason", "validator_report.policy_checks[]")?
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;

    // Tests (same as v1).
    let tests = obj
        .get("tests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.tests: expected array"))?
        .iter()
        .map(|item| {
            let o = require_object(item, "validator_report.tests[]")?;
            Ok(json!({
                "name": require_string(o, "name", "validator_report.tests[]")?,
                "pass": o.get("pass").and_then(|v| v.as_bool()).ok_or_else(|| ai_error("INVALID_INPUT", "validator_report.tests[].pass is required"))?,
                "evidence_ref": require_string(o, "evidence_ref", "validator_report.tests[]")?
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;

    // Security findings with validated severity.
    let security_findings_raw = obj
        .get("security_findings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "validator_report.security_findings: expected array",
            )
        })?;

    let mut security_findings = Vec::new();
    for (idx, item) in security_findings_raw.iter().enumerate() {
        let sf = require_object(item, &format!("validator_report.security_findings[{idx}]"))?;
        let severity = require_string(
            sf,
            "severity",
            &format!("validator_report.security_findings[{idx}]"),
        )?
        .to_ascii_lowercase();
        if !VALID_SECURITY_SEVERITIES.contains(&severity.as_str()) {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "validator_report.security_findings[{idx}].severity must be one of: {}",
                    VALID_SECURITY_SEVERITIES.join("|")
                ),
            ));
        }
        let description = require_string(
            sf,
            "description",
            &format!("validator_report.security_findings[{idx}]"),
        )?;
        let affected_file = require_string(
            sf,
            "affected_file",
            &format!("validator_report.security_findings[{idx}]"),
        )?;
        security_findings.push(json!({
            "severity": severity,
            "description": description,
            "affected_file": affected_file
        }));
    }

    let regression_risk =
        require_string(obj, "regression_risk", "validator_report")?.to_ascii_lowercase();
    if !matches!(regression_risk.as_str(), "low" | "medium" | "high") {
        return Err(ai_error(
            "INVALID_INPUT",
            "validator_report.regression_risk must be low|medium|high",
        ));
    }

    // v2 recommendation: approve|writer_retry|scout_retry|escalate.
    let recommendation =
        require_string(obj, "recommendation", "validator_report")?.to_ascii_lowercase();
    let valid_recs = [
        "approve",
        "rework",
        "reject",
        "writer_retry",
        "scout_retry",
        "escalate",
    ];
    if !valid_recs.contains(&recommendation.as_str()) {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!(
                "validator_report.recommendation must be one of: {}",
                valid_recs.join("|")
            ),
        ));
    }

    let rework_actions = string_array(obj, "rework_actions", "validator_report")?;

    // rework_actions must not be empty when retry/rework.
    if matches!(
        recommendation.as_str(),
        "rework" | "writer_retry" | "scout_retry"
    ) && rework_actions.is_empty()
    {
        return Err(ai_error(
            "INVALID_INPUT",
            "validator_report.rework_actions must not be empty when recommendation is rework/retry",
        ));
    }

    // Traceability warning (soft check — logged but not blocking).
    let mut warnings = Vec::new();
    if traceability_ratio < MIN_TRACEABILITY_RATIO {
        warnings.push(format!(
            "traceability_ratio {traceability_ratio:.2} < threshold {MIN_TRACEABILITY_RATIO:.2}"
        ));
    }

    Ok(ValidatorReportContract {
        recommendation: recommendation.clone(),
        normalized: json!({
            "slice_id": slice_id,
            "intent_compliance": {
                "aspects_requested": aspects_requested,
                "aspects_fulfilled": aspects_fulfilled,
                "aspects_missing": aspects_missing,
                "aspects_unexpected": aspects_unexpected,
                "fulfillment_ratio": fulfillment_ratio
            },
            "traceability": {
                "writer_refs_to_scout_anchors": writer_refs,
                "untraced_changes": untraced,
                "traceability_ratio": traceability_ratio
            },
            "policy_checks": policy_checks,
            "tests": tests,
            "security_findings": security_findings,
            "regression_risk": regression_risk,
            "recommendation": recommendation,
            "rework_actions": rework_actions,
            "warnings": warnings
        }),
    })
}

/// Cross-validate writer output against scout scope.
/// Returns list of violations (empty = pass).
pub(crate) fn cross_validate_writer_scout(
    writer_affected_files: &[String],
    scout_scope_in: &[String],
    scout_change_hint_paths: &[String],
) -> Vec<String> {
    let mut violations = Vec::new();
    let allowed: HashSet<&str> = scout_scope_in
        .iter()
        .chain(scout_change_hint_paths.iter())
        .map(|s| s.as_str())
        .collect();

    for file in writer_affected_files {
        if !allowed.contains(file.as_str()) {
            violations.push(format!(
                "writer modified '{file}' which is outside scout scope"
            ));
        }
    }
    violations
}

// ── Cascade Pipeline Session ──

const CASCADE_MAX_SCOUT_RETRIES: u32 = 2;
const CASCADE_MAX_WRITER_RETRIES: u32 = 2;
const CASCADE_MAX_SCOUT_RERUNS: u32 = 1;
const CASCADE_MAX_TOTAL_LLM_CALLS: u32 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CascadePhase {
    Scout,
    PreValidate,
    Writer,
    PostValidate,
    Apply,
    Escalated,
}

impl CascadePhase {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Scout => "scout",
            Self::PreValidate => "pre_validate",
            Self::Writer => "writer",
            Self::PostValidate => "post_validate",
            Self::Apply => "apply",
            Self::Escalated => "escalated",
        }
    }

    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "scout" => Some(Self::Scout),
            "pre_validate" => Some(Self::PreValidate),
            "writer" => Some(Self::Writer),
            "post_validate" => Some(Self::PostValidate),
            "apply" => Some(Self::Apply),
            "escalated" => Some(Self::Escalated),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CascadeSession {
    pub session_id: String,
    pub phase: CascadePhase,
    pub scout_retries: u32,
    pub writer_retries: u32,
    pub scout_reruns: u32,
    pub total_llm_calls: u32,
    pub scout_job_ids: Vec<String>,
    pub writer_job_ids: Vec<String>,
    pub validator_job_ids: Vec<String>,
}

impl CascadeSession {
    pub(crate) fn new(session_id: String) -> Self {
        Self {
            session_id,
            phase: CascadePhase::Scout,
            scout_retries: 0,
            writer_retries: 0,
            scout_reruns: 0,
            total_llm_calls: 0,
            scout_job_ids: Vec::new(),
            writer_job_ids: Vec::new(),
            validator_job_ids: Vec::new(),
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "session_id": self.session_id,
            "phase": self.phase.as_str(),
            "scout_retries": self.scout_retries,
            "writer_retries": self.writer_retries,
            "scout_reruns": self.scout_reruns,
            "total_llm_calls": self.total_llm_calls,
            "lineage": {
                "scout_job_ids": self.scout_job_ids,
                "writer_job_ids": self.writer_job_ids,
                "validator_job_ids": self.validator_job_ids
            }
        })
    }

    pub(crate) fn from_json(val: &Value) -> Option<Self> {
        let session_id = val.get("session_id")?.as_str()?.to_string();
        let phase_str = val.get("phase")?.as_str()?;
        let phase = CascadePhase::from_str(phase_str)?;
        let scout_retries = val.get("scout_retries")?.as_u64()? as u32;
        let writer_retries = val.get("writer_retries")?.as_u64()? as u32;
        let scout_reruns = val.get("scout_reruns")?.as_u64()? as u32;
        let total_llm_calls = val.get("total_llm_calls")?.as_u64()? as u32;
        let lineage = val.get("lineage")?;
        let scout_job_ids = lineage
            .get("scout_job_ids")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let writer_job_ids = lineage
            .get("writer_job_ids")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let validator_job_ids = lineage
            .get("validator_job_ids")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        Some(Self {
            session_id,
            phase,
            scout_retries,
            writer_retries,
            scout_reruns,
            total_llm_calls,
            scout_job_ids,
            writer_job_ids,
            validator_job_ids,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CascadeAction {
    RunPreValidate,
    DispatchWriter,
    DispatchValidator,
    ApplyResult,
    RetryScout { hints: Vec<String> },
    RetryWriter { feedback: Vec<String> },
    RerunScout { missing: Vec<String> },
    Escalate { reason: String },
}

/// Advance the cascade state machine.
/// `event` is the trigger (e.g. "scout_done", "pre_validate_pass", "writer_done", "approve").
/// `hints` carries feedback from the validator/pre-validator.
pub(crate) fn cascade_advance(
    session: &mut CascadeSession,
    event: &str,
    hints: Vec<String>,
) -> CascadeAction {
    match (session.phase.as_str(), event) {
        // Scout phase.
        ("scout", "scout_done") => {
            session.phase = CascadePhase::PreValidate;
            CascadeAction::RunPreValidate
        }

        // Pre-validate phase.
        ("pre_validate", "pre_validate_pass") => {
            session.phase = CascadePhase::Writer;
            session.total_llm_calls += 1;
            CascadeAction::DispatchWriter
        }
        ("pre_validate", "pre_validate_need_more") => {
            if session.total_llm_calls >= CASCADE_MAX_TOTAL_LLM_CALLS
                || session.scout_retries >= CASCADE_MAX_SCOUT_RETRIES
            {
                session.phase = CascadePhase::Escalated;
                return CascadeAction::Escalate {
                    reason: "scout retries exhausted".into(),
                };
            }
            session.scout_retries += 1;
            session.total_llm_calls += 1;
            session.phase = CascadePhase::Scout;
            CascadeAction::RetryScout { hints }
        }
        ("pre_validate", "pre_validate_reject") => {
            session.phase = CascadePhase::Escalated;
            CascadeAction::Escalate {
                reason: format!("pre-validator rejected: {}", hints.join("; ")),
            }
        }

        // Writer phase.
        ("writer", "writer_done") => {
            session.phase = CascadePhase::PostValidate;
            session.total_llm_calls += 1;
            CascadeAction::DispatchValidator
        }

        // Post-validate phase.
        ("post_validate", "approve") => {
            session.phase = CascadePhase::Apply;
            CascadeAction::ApplyResult
        }
        ("post_validate", "writer_retry") => {
            if session.total_llm_calls >= CASCADE_MAX_TOTAL_LLM_CALLS
                || session.writer_retries >= CASCADE_MAX_WRITER_RETRIES
            {
                session.phase = CascadePhase::Escalated;
                return CascadeAction::Escalate {
                    reason: "writer retries exhausted".into(),
                };
            }
            session.writer_retries += 1;
            session.phase = CascadePhase::Writer;
            session.total_llm_calls += 1;
            CascadeAction::RetryWriter { feedback: hints }
        }
        ("post_validate", "scout_retry") => {
            if session.total_llm_calls >= CASCADE_MAX_TOTAL_LLM_CALLS
                || session.scout_reruns >= CASCADE_MAX_SCOUT_RERUNS
            {
                session.phase = CascadePhase::Escalated;
                return CascadeAction::Escalate {
                    reason: "scout reruns exhausted".into(),
                };
            }
            session.scout_reruns += 1;
            session.total_llm_calls += 1;
            session.phase = CascadePhase::Scout;
            CascadeAction::RerunScout { missing: hints }
        }
        ("post_validate", "escalate") | ("post_validate", "reject") => {
            session.phase = CascadePhase::Escalated;
            CascadeAction::Escalate {
                reason: format!("post-validator escalated: {}", hints.join("; ")),
            }
        }

        // Unknown event.
        (_phase, unknown_event) => {
            let phase_str = session.phase.as_str().to_string();
            session.phase = CascadePhase::Escalated;
            CascadeAction::Escalate {
                reason: format!(
                    "unexpected event '{}' in phase '{}'",
                    unknown_event, phase_str
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_anchor(id: &str, typ: ScoutAnchorType, code_ref: &str, content: &str) -> ScoutAnchorV2 {
        ScoutAnchorV2 {
            id: id.to_string(),
            anchor_type: typ,
            rationale: format!("{id} rationale"),
            code_ref: code_ref.to_string(),
            content: content.to_string(),
            line_count: content.lines().count() as u32,
            meta_hint: None,
        }
    }

    fn full_scout_pack(anchors: &[ScoutAnchorV2]) -> Value {
        let anchors_json: Vec<Value> = anchors
            .iter()
            .map(|a| {
                json!({
                    "id": a.id,
                    "anchor_type": a.anchor_type.as_str(),
                    "rationale": a.rationale,
                    "code_ref": a.code_ref,
                    "content": a.content,
                    "line_count": a.line_count,
                })
            })
            .collect();
        json!({
            "format_version": 2,
            "objective": "Add rate limiting to API",
            "scope": { "in": ["src/routes.rs", "src/middleware.rs"], "out": [] },
            "anchors": anchors_json,
            "change_hints": [
                { "path": "src/routes.rs", "intent": "add limiter", "risk": "medium" },
                { "path": "src/middleware.rs", "intent": "extract middleware", "risk": "low" }
            ],
            "test_hints": ["cargo test"],
            "risk_map": [{ "risk": "regression", "falsifier": "smoke" }],
            "open_questions": [],
            "summary_for_builder": "Add rate limiter middleware",
            "coverage_matrix": {
                "objective_items": ["rate limiting"],
                "dod_items": ["tests pass"],
                "tests_map": [],
                "risks_map": [],
                "unknowns": []
            }
        })
    }

    #[test]
    fn writer_patch_pack_accepts_minimal_replace() {
        let raw = json!({
            "slice_id": "SLC-1",
            "summary": "replace one section",
            "affected_files": ["src/lib.rs"],
            "patches": [
                {
                    "path": "src/lib.rs",
                    "ops": [
                        {
                            "kind": "replace",
                            "old_lines": ["old"],
                            "new_lines": ["new"]
                        }
                    ]
                }
            ],
            "checks_to_run": ["cargo test -p bm_mcp --test pipeline_v2_integration"]
        });
        let normalized = validate_writer_patch_pack(&raw).expect("writer pack must validate");
        assert_eq!(
            normalized
                .get("slice_id")
                .and_then(|v| v.as_str())
                .expect("slice_id"),
            "SLC-1"
        );
    }

    #[test]
    fn writer_patch_pack_rejects_empty_without_escape_hatch() {
        let raw = json!({
            "slice_id": "SLC-1",
            "summary": "empty",
            "affected_files": [],
            "patches": [],
            "checks_to_run": []
        });
        let err = validate_writer_patch_pack(&raw).expect_err("must reject empty patches");
        assert_eq!(
            err.get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str()),
            Some("INVALID_INPUT")
        );
    }

    #[test]
    fn pre_validator_pass_full_pack() {
        let anchors = vec![
            make_anchor(
                "a:routes-handler",
                ScoutAnchorType::Primary,
                "code:src/routes.rs#L10-L30@sha256:aaa",
                "pub fn create_user() {}",
            ),
            make_anchor(
                "a:middleware-base",
                ScoutAnchorType::Primary,
                "code:src/middleware.rs#L1-L20@sha256:bbb",
                "pub struct Middleware;",
            ),
            make_anchor(
                "a:types-dep",
                ScoutAnchorType::Dependency,
                "code:src/types.rs#L1-L10@sha256:ccc",
                "pub struct Request;",
            ),
            make_anchor(
                "a:style-ref",
                ScoutAnchorType::Reference,
                "code:src/existing.rs#L1-L5@sha256:ddd",
                "// existing pattern",
            ),
        ];
        let pack = full_scout_pack(&anchors);
        let (verdict, checks) = pre_validate_scout_pack(&pack, &anchors);
        assert_eq!(verdict, PreValidatorVerdict::Pass);
        assert!(checks.completeness_ok);
        assert!(checks.dependencies_ok);
        assert!(checks.patterns_ok);
        assert!(checks.intent_coverage_ok);
    }

    #[test]
    fn pre_validator_need_more_missing_primary() {
        let anchors = vec![
            make_anchor(
                "a:routes-handler",
                ScoutAnchorType::Primary,
                "code:src/routes.rs#L10-L30@sha256:aaa",
                "pub fn create_user() {}",
            ),
            make_anchor(
                "a:dep",
                ScoutAnchorType::Dependency,
                "code:src/types.rs#L1-L5@sha256:bbb",
                "struct X;",
            ),
            make_anchor(
                "a:ref",
                ScoutAnchorType::Reference,
                "code:src/lib.rs#L1-L3@sha256:ccc",
                "// style",
            ),
        ];
        let pack = full_scout_pack(&anchors);
        let (verdict, checks) = pre_validate_scout_pack(&pack, &anchors);
        assert!(!checks.completeness_ok);
        match verdict {
            PreValidatorVerdict::NeedMore { hints } => {
                assert!(hints.iter().any(|h| h.contains("middleware.rs")));
            }
            other => panic!("expected NeedMore, got: {other:?}"),
        }
    }

    #[test]
    fn pre_validator_reject_empty_anchors() {
        let pack = full_scout_pack(&[]);
        let (verdict, _checks) = pre_validate_scout_pack(&pack, &[]);
        match verdict {
            PreValidatorVerdict::Reject { reason } => {
                assert!(reason.contains("zero anchors"));
            }
            other => panic!("expected Reject, got: {other:?}"),
        }
    }

    #[test]
    fn pre_validator_synthesizes_legacy_anchor_shape_from_code_refs() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let pack = json!({
            "objective": "Legacy scout pack without typed anchors",
            "scope": { "in": ["src/routes.rs"], "out": [] },
            "anchors": [
                { "id": "a:legacy-1", "rationale": "Legacy primary anchor rationale" },
                { "id": "a:legacy-2", "rationale": "Legacy dependency anchor rationale" },
                { "id": "a:legacy-3", "rationale": "Legacy reference anchor rationale" }
            ],
            "code_refs": [
                format!("code:src/routes.rs#L10-L30@sha256:{sha}"),
                format!("code:src/middleware.rs#L1-L20@sha256:{sha}"),
                format!("code:src/patterns.rs#L5-L15@sha256:{sha}")
            ],
            "change_hints": [
                { "path": "src/routes.rs", "intent": "add limiter", "risk": "medium" },
                { "path": "src/middleware.rs", "intent": "wire middleware", "risk": "low" }
            ],
            "test_hints": ["cargo test"],
            "risk_map": [{ "risk": "regression", "falsifier": "smoke" }],
            "open_questions": [],
            "summary_for_builder": "legacy",
            "coverage_matrix": {
                "objective_items": ["rate limiting"],
                "dod_items": ["tests pass"],
                "tests_map": [],
                "risks_map": [],
                "unknowns": []
            }
        });

        let (verdict, checks) = pre_validate_scout_pack(&pack, &[]);
        assert!(
            !matches!(verdict, PreValidatorVerdict::Reject { .. }),
            "legacy anchors should not hard-reject as zero anchors: {verdict:?}"
        );
        assert!(
            checks.completeness_ok,
            "legacy fallback must cover change_hints via synthesized anchors"
        );
    }

    #[test]
    fn pre_validator_need_more_no_reference() {
        let anchors = vec![
            make_anchor(
                "a:r1",
                ScoutAnchorType::Primary,
                "code:src/routes.rs#L10-L30@sha256:aaa",
                "fn handler() {}",
            ),
            make_anchor(
                "a:m1",
                ScoutAnchorType::Primary,
                "code:src/middleware.rs#L1-L10@sha256:bbb",
                "struct M;",
            ),
            make_anchor(
                "a:dep",
                ScoutAnchorType::Dependency,
                "code:src/types.rs#L1-L5@sha256:ccc",
                "struct T;",
            ),
        ];
        let pack = full_scout_pack(&anchors);
        let (verdict, checks) = pre_validate_scout_pack(&pack, &anchors);
        assert!(!checks.patterns_ok);
        match verdict {
            PreValidatorVerdict::NeedMore { hints } => {
                assert!(hints.iter().any(|h| h.contains("reference")));
            }
            other => panic!("expected NeedMore, got: {other:?}"),
        }
    }

    #[test]
    fn pre_validator_need_more_no_dependencies() {
        let anchors = vec![
            make_anchor(
                "a:r1",
                ScoutAnchorType::Primary,
                "code:src/routes.rs#L10-L30@sha256:aaa",
                "fn handler() {}",
            ),
            make_anchor(
                "a:m1",
                ScoutAnchorType::Structural,
                "code:src/middleware.rs#L1-L10@sha256:bbb",
                "mod middleware;",
            ),
            make_anchor(
                "a:ref",
                ScoutAnchorType::Reference,
                "code:src/lib.rs#L1-L3@sha256:ccc",
                "// style",
            ),
        ];
        let pack = full_scout_pack(&anchors);
        let (verdict, checks) = pre_validate_scout_pack(&pack, &anchors);
        assert!(!checks.dependencies_ok);
        assert!(!checks.dependencies_missing.is_empty());
        match verdict {
            PreValidatorVerdict::Pass => {}
            other => panic!("expected Pass, got: {other:?}"),
        }
    }

    // ── Post-Validator v2 tests ──

    fn valid_v2_report() -> Value {
        json!({
            "slice_id": "SLICE-001",
            "intent_compliance": {
                "aspects_requested": ["rate limiting", "error handling"],
                "aspects_fulfilled": ["rate limiting", "error handling"],
                "aspects_missing": [],
                "fulfillment_ratio": 1.0
            },
            "traceability": {
                "writer_refs_to_scout_anchors": ["a:routes-handler", "a:middleware-base"],
                "untraced_changes": [],
                "traceability_ratio": 1.0
            },
            "policy_checks": [{ "name": "no_direct_fs_writes", "pass": true, "reason": "all changes via PatchOp" }],
            "tests": [{ "name": "cargo test", "pass": true, "evidence_ref": "CMD: cargo test -q" }],
            "security_findings": [],
            "regression_risk": "low",
            "recommendation": "approve",
            "rework_actions": []
        })
    }

    #[test]
    fn validator_v2_accepts_valid_report() {
        let contract = validate_validator_report_v2(&valid_v2_report()).expect("must accept");
        assert_eq!(contract.recommendation, "approve");
    }

    #[test]
    fn validator_v2_falls_back_to_v1_without_intent() {
        let report = json!({
            "slice_id": "S1",
            "plan_fit_score": 85,
            "policy_checks": [{"name": "check", "pass": true, "reason": "ok"}],
            "tests": [{"name": "test", "pass": true, "evidence_ref": "CMD: test"}],
            "security_findings": [],
            "regression_risk": "low",
            "recommendation": "approve",
            "rework_actions": []
        });
        let contract = validate_validator_report_v2(&report).expect("must accept v1");
        assert_eq!(contract.recommendation, "approve");
    }

    #[test]
    fn validator_v2_rejects_invalid_severity() {
        let mut report = valid_v2_report();
        report["security_findings"] = json!([{ "severity": "URGENT", "description": "SQL injection", "affected_file": "src/db.rs" }]);
        let err = validate_validator_report_v2(&report).expect_err("must reject");
        assert!(err.to_string().contains("severity"), "got: {err}");
    }

    #[test]
    fn validator_v2_accepts_valid_severity_levels() {
        for severity in &["critical", "high", "medium", "low", "info"] {
            let mut report = valid_v2_report();
            report["security_findings"] = json!([{ "severity": severity, "description": "finding", "affected_file": "src/lib.rs" }]);
            validate_validator_report_v2(&report)
                .unwrap_or_else(|e| panic!("must accept severity '{severity}': {e}"));
        }
    }

    #[test]
    fn validator_v2_requires_rework_actions_for_retry() {
        for rec in &["writer_retry", "scout_retry", "rework"] {
            let mut report = valid_v2_report();
            report["recommendation"] = json!(rec);
            report["rework_actions"] = json!([]);
            let err = validate_validator_report_v2(&report)
                .expect_err(&format!("must reject {rec} without rework_actions"));
            assert!(
                err.to_string().contains("rework_actions"),
                "{rec}: got: {err}"
            );
        }
    }

    #[test]
    fn validator_v2_accepts_retry_with_actions() {
        let mut report = valid_v2_report();
        report["recommendation"] = json!("writer_retry");
        report["rework_actions"] = json!(["fix tests", "add error handling"]);
        let contract =
            validate_validator_report_v2(&report).expect("must accept writer_retry with actions");
        assert_eq!(contract.recommendation, "writer_retry");
    }

    // ── Cross-validation tests ──

    #[test]
    fn cross_validate_pass_within_scope() {
        let violations = cross_validate_writer_scout(
            &["src/routes.rs".into(), "src/middleware.rs".into()],
            &["src/routes.rs".into(), "src/middleware.rs".into()],
            &["src/routes.rs".into(), "src/middleware.rs".into()],
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn cross_validate_detects_out_of_scope() {
        let violations = cross_validate_writer_scout(
            &["src/routes.rs".into(), "src/secret.rs".into()],
            &["src/routes.rs".into()],
            &["src/routes.rs".into()],
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("secret.rs"));
    }

    // ── Cascade pipeline tests ──

    #[test]
    fn cascade_happy_path() {
        let mut session = CascadeSession::new("pls-test1234".into());
        assert_eq!(session.phase, CascadePhase::Scout);
        let action = cascade_advance(&mut session, "scout_done", vec![]);
        assert_eq!(action, CascadeAction::RunPreValidate);
        assert_eq!(session.phase, CascadePhase::PreValidate);
        let action = cascade_advance(&mut session, "pre_validate_pass", vec![]);
        assert_eq!(action, CascadeAction::DispatchWriter);
        assert_eq!(session.phase, CascadePhase::Writer);
        assert_eq!(session.total_llm_calls, 1);
        let action = cascade_advance(&mut session, "writer_done", vec![]);
        assert_eq!(action, CascadeAction::DispatchValidator);
        assert_eq!(session.phase, CascadePhase::PostValidate);
        assert_eq!(session.total_llm_calls, 2);
        let action = cascade_advance(&mut session, "approve", vec![]);
        assert_eq!(action, CascadeAction::ApplyResult);
        assert_eq!(session.phase, CascadePhase::Apply);
    }

    #[test]
    fn cascade_scout_retry_then_pass() {
        let mut session = CascadeSession::new("pls-retry001".into());
        cascade_advance(&mut session, "scout_done", vec![]);
        let action = cascade_advance(
            &mut session,
            "pre_validate_need_more",
            vec!["missing coverage for AuthService".into()],
        );
        match action {
            CascadeAction::RetryScout { hints } => assert!(hints[0].contains("AuthService")),
            other => panic!("expected RetryScout, got: {other:?}"),
        }
        assert_eq!(session.phase, CascadePhase::Scout);
        assert_eq!(session.scout_retries, 1);
        cascade_advance(&mut session, "scout_done", vec![]);
        let action = cascade_advance(&mut session, "pre_validate_pass", vec![]);
        assert_eq!(action, CascadeAction::DispatchWriter);
    }

    #[test]
    fn cascade_writer_retry_exhausted_escalates() {
        let mut session = CascadeSession::new("pls-exhaust1".into());
        cascade_advance(&mut session, "scout_done", vec![]);
        cascade_advance(&mut session, "pre_validate_pass", vec![]);
        cascade_advance(&mut session, "writer_done", vec![]);
        cascade_advance(&mut session, "writer_retry", vec!["fix tests".into()]);
        assert_eq!(session.writer_retries, 1);
        session.phase = CascadePhase::PostValidate;
        cascade_advance(&mut session, "writer_retry", vec!["still broken".into()]);
        assert_eq!(session.writer_retries, 2);
        session.phase = CascadePhase::PostValidate;
        let action = cascade_advance(&mut session, "writer_retry", vec!["gave up".into()]);
        match action {
            CascadeAction::Escalate { reason } => {
                assert!(reason.contains("writer retries exhausted"))
            }
            other => panic!("expected Escalate, got: {other:?}"),
        }
        assert_eq!(session.phase, CascadePhase::Escalated);
    }

    #[test]
    fn cascade_total_llm_limit_escalates() {
        let mut session = CascadeSession::new("pls-limit001".into());
        session.total_llm_calls = CASCADE_MAX_TOTAL_LLM_CALLS;
        cascade_advance(&mut session, "scout_done", vec![]);
        let action = cascade_advance(
            &mut session,
            "pre_validate_need_more",
            vec!["more context needed".into()],
        );
        match action {
            CascadeAction::Escalate { reason } => {
                assert!(reason.contains("scout retries exhausted"))
            }
            other => panic!("expected Escalate, got: {other:?}"),
        }
    }

    #[test]
    fn cascade_session_json_roundtrip() {
        let mut session = CascadeSession::new("pls-rt123456".into());
        session.phase = CascadePhase::Writer;
        session.scout_retries = 1;
        session.total_llm_calls = 3;
        session.scout_job_ids = vec!["JOB-1".into(), "JOB-2".into()];
        session.writer_job_ids = vec!["JOB-3".into()];
        let json = session.to_json();
        let restored = CascadeSession::from_json(&json).expect("must parse");
        assert_eq!(restored.session_id, "pls-rt123456");
        assert_eq!(restored.phase, CascadePhase::Writer);
        assert_eq!(restored.scout_retries, 1);
        assert_eq!(restored.total_llm_calls, 3);
        assert_eq!(restored.scout_job_ids, vec!["JOB-1", "JOB-2"]);
        assert_eq!(restored.writer_job_ids, vec!["JOB-3"]);
    }

    #[test]
    fn cascade_scout_rerun_from_post_validator() {
        let mut session = CascadeSession::new("pls-rerun001".into());
        cascade_advance(&mut session, "scout_done", vec![]);
        cascade_advance(&mut session, "pre_validate_pass", vec![]);
        cascade_advance(&mut session, "writer_done", vec![]);
        let action = cascade_advance(
            &mut session,
            "scout_retry",
            vec!["missing error handling context".into()],
        );
        match action {
            CascadeAction::RerunScout { missing } => assert!(missing[0].contains("error handling")),
            other => panic!("expected RerunScout, got: {other:?}"),
        }
        assert_eq!(session.scout_reruns, 1);
        assert_eq!(session.phase, CascadePhase::Scout);
        cascade_advance(&mut session, "scout_done", vec![]);
        cascade_advance(&mut session, "pre_validate_pass", vec![]);
        cascade_advance(&mut session, "writer_done", vec![]);
        let action = cascade_advance(&mut session, "scout_retry", vec!["still not enough".into()]);
        match action {
            CascadeAction::Escalate { reason } => {
                assert!(reason.contains("scout reruns exhausted"))
            }
            other => panic!("expected Escalate, got: {other:?}"),
        }
    }
}
