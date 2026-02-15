#![forbid(unsafe_code)]

use super::super::*;
use super::DEFAULT_CONTEXT_REVIEW_MODE;
use super::{
    optional_non_empty_string, parse_meta_map, require_non_empty_string, scout_policy_from_meta,
};
use crate::support::{
    ensure_artifact_ref, extract_job_id_from_ref, parse_json_object_from_text,
    pre_validate_scout_pack, validate_scout_context_pack as validate_scout_context_pack_contract,
};
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_context_review(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "slice_id",
                "scout_pack_ref",
                "mode",
                "policy",
            ],
            "jobs.pipeline.context.review",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task_id = match require_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match require_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mode = match optional_non_empty_string(args_obj, "mode") {
            Ok(v) => v
                .unwrap_or_else(|| DEFAULT_CONTEXT_REVIEW_MODE.to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if !matches!(mode.as_str(), "deterministic" | "haiku_fast") {
            return ai_error(
                "INVALID_INPUT",
                "jobs.pipeline.context.review: mode must be deterministic|haiku_fast",
            );
        }
        let policy = match optional_non_empty_string(args_obj, "policy") {
            Ok(v) => v
                .unwrap_or_else(|| "fail_closed".to_string())
                .to_ascii_lowercase(),
            Err(resp) => return resp,
        };
        if policy != "fail_closed" {
            return ai_error(
                "INVALID_INPUT",
                "jobs.pipeline.context.review: policy must be fail_closed",
            );
        }
        let scout_pack_ref = match args_obj.get("scout_pack_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "scout_pack_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "scout_pack_ref is required"),
        };
        let scout_job_id = match extract_job_id_from_ref(&scout_pack_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "scout_pack_ref must include a JOB-... lineage token",
                );
            }
        };

        let scout_open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: scout_job_id.clone(),
                include_prompt: false,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => {
                return ai_error("UNKNOWN_ID", "Unknown scout job id from scout_pack_ref");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !scout_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.context.review: scout job is not DONE",
            );
        }

        let scout_summary = match scout_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.context.review: scout job summary is empty",
                );
            }
        };
        let scout_json = match parse_json_object_from_text(
            scout_summary,
            "scout summary (scout_context_pack)",
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let scout_meta = parse_meta_map(scout_open.meta_json.as_deref());
        let scout_max_context_refs = scout_meta
            .get("max_context_refs")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(24)
            .clamp(8, 64);
        let scout_policy = scout_policy_from_meta(&scout_meta);

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let (scout_norm, scout_contract_warnings) = match validate_scout_context_pack_contract(
            &self.store,
            &workspace,
            &scout_json,
            scout_max_context_refs,
            &scout_policy,
        ) {
            Ok(v) => v,
            Err(resp) => {
                let reason = resp
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("scout pack failed strict contract")
                    .to_string();
                let result = json!({
                    "workspace": workspace.as_str(),
                    "task": task_id,
                    "slice_id": slice_id,
                    "scout_pack_ref": scout_pack_ref,
                    "mode": mode,
                    "policy": policy,
                    "scores": {
                        "freshness": 0.0,
                        "coverage": 0.0,
                        "dedupe": 0.0,
                        "traceability": 0.0,
                        "semantic_cohesion": 0.0
                    },
                    "verdict": { "status": "reject", "reason": reason },
                    "checks": {
                        "completeness_ok": false,
                        "completeness_missing": ["strict scout contract failed"],
                        "dependencies_ok": false,
                        "dependencies_missing": [],
                        "patterns_ok": false,
                        "patterns_missing": [],
                        "intent_coverage_ok": false,
                        "intent_coverage_missing": []
                    },
                    "missing_context": ["refresh scout contract output"],
                    "actions": []
                });
                return if warnings.is_empty() {
                    ai_ok("tasks_jobs_pipeline_context_review", result)
                } else {
                    ai_ok_with_warnings(
                        "tasks_jobs_pipeline_context_review",
                        result,
                        warnings,
                        Vec::new(),
                    )
                };
            }
        };
        warnings.extend(scout_contract_warnings.clone());

        let anchors_v2: Vec<ScoutAnchorV2> = scout_norm
            .get("anchors")
            .and_then(|v| v.as_array())
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let id = obj.get("id")?.as_str()?.to_string();
                let anchor_type =
                    ScoutAnchorType::from_str(obj.get("anchor_type")?.as_str()?).ok()?;
                let rationale = obj
                    .get("rationale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let code_ref = obj
                    .get("code_ref")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let line_count = obj.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let meta_hint = obj
                    .get("meta_hint")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Some(ScoutAnchorV2 {
                    id,
                    anchor_type,
                    rationale,
                    code_ref,
                    content,
                    line_count,
                    meta_hint,
                })
            })
            .collect();

        let (verdict, checks) = pre_validate_scout_pack(&scout_norm, &anchors_v2);
        let has_stale = scout_contract_warnings.iter().any(|w| {
            matches!(
                w.get("code").and_then(|v| v.as_str()),
                Some("CODE_REF_STALE" | "CODE_REF_MISSING" | "CODE_REF_RANGE_STALE")
            )
        });
        let has_novelty_warn = scout_contract_warnings.iter().any(|w| {
            w.get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "SCOUT_NOVELTY_WARN")
        });
        let any_warning = scout_contract_warnings.iter().any(|w| {
            !matches!(
                w.get("code").and_then(|v| v.as_str()),
                Some("CODE_REF_UNRESOLVABLE")
            )
        });

        let coverage_score = [
            checks.completeness_ok,
            checks.dependencies_ok,
            checks.patterns_ok,
            checks.intent_coverage_ok,
        ]
        .into_iter()
        .filter(|ok| *ok)
        .count() as f64
            / 4.0;
        let freshness_score = if has_stale { 0.0 } else { 1.0 };
        let dedupe_score = if has_novelty_warn { 0.5 } else { 1.0 };
        let traceability_score = if anchors_v2.len() >= 3 { 1.0 } else { 0.0 };
        let summary_len = scout_norm
            .get("summary_for_builder")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let has_architecture_map = scout_norm.get("architecture_map").is_some();
        let has_mermaid = scout_norm
            .get("mermaid_compact")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        let mut semantic_cohesion: f64 = if summary_len >= 320 { 0.7 } else { 0.4 };
        if has_architecture_map {
            semantic_cohesion += 0.2;
        }
        if has_mermaid {
            semantic_cohesion += 0.1;
        }
        if mode == "deterministic" {
            semantic_cohesion = (semantic_cohesion - 0.05).max(0.0);
        }
        semantic_cohesion = semantic_cohesion.min(1.0);

        let (verdict_status, verdict_reason, missing_context): (
            String,
            Option<String>,
            Vec<String>,
        ) = if any_warning {
            let reason = scout_contract_warnings
                .first()
                .and_then(|w| w.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("scout context warning in fail_closed policy")
                .to_string();
            let mut missing = vec![reason.clone()];
            if has_stale {
                missing.push("refresh CODE_REF sha256 + line ranges".to_string());
            }
            ("reject".to_string(), Some(reason), missing)
        } else {
            match verdict {
                crate::support::PreValidatorVerdict::Pass => ("pass".to_string(), None, Vec::new()),
                crate::support::PreValidatorVerdict::NeedMore { hints } => {
                    let missing = if hints.is_empty() {
                        vec!["pre-validate need_more with no hints".to_string()]
                    } else {
                        hints
                    };
                    (
                        "need_more".to_string(),
                        Some("pre-validate requested more context".to_string()),
                        missing,
                    )
                }
                crate::support::PreValidatorVerdict::Reject { reason } => {
                    ("reject".to_string(), Some(reason.clone()), vec![reason])
                }
            }
        };

        let mut actions = Vec::<Value>::new();
        if verdict_status == "pass" {
            actions.push(json!({
                "cmd": "jobs.macro.dispatch.builder",
                "args": {
                    "task": task_id.clone(),
                    "slice_id": slice_id.clone(),
                    "scout_pack_ref": scout_pack_ref.clone(),
                    "objective": "implement from reviewed scout context",
                    "dod": {"criteria": [], "tests": [], "security": []},
                    "executor": "codex",
                    "executor_profile": "xhigh",
                    "strict_scout_mode": true,
                    "context_quality_gate": true,
                    "input_mode": "strict"
                }
            }));
        } else if let Some(anchor) = scout_open.job.anchor_id.clone() {
            actions.push(json!({
            "cmd": "jobs.macro.dispatch.scout",
            "args": {
                "task": task_id.clone(),
                "anchor": anchor,
                "slice_id": slice_id.clone(),
                "objective": format!("context refresh requested: {}", verdict_reason.clone().unwrap_or_else(|| "context gap".to_string())),
                "constraints": missing_context.clone(),
                "quality_profile": "flagship",
                "novelty_policy": "strict"
            }
        }));
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "task": task_id,
            "slice_id": slice_id,
            "scout_pack_ref": scout_pack_ref,
            "mode": mode,
            "policy": policy,
            "scores": {
                "freshness": freshness_score,
                "coverage": coverage_score,
                "dedupe": dedupe_score,
                "traceability": traceability_score,
                "semantic_cohesion": semantic_cohesion
            },
            "verdict": {
                "status": verdict_status,
                "reason": verdict_reason
            },
            "checks": {
                "completeness_ok": checks.completeness_ok,
                "completeness_missing": checks.completeness_missing,
                "dependencies_ok": checks.dependencies_ok,
                "dependencies_missing": checks.dependencies_missing,
                "patterns_ok": checks.patterns_ok,
                "patterns_missing": checks.patterns_missing,
                "intent_coverage_ok": checks.intent_coverage_ok,
                "intent_coverage_missing": checks.intent_coverage_missing
            },
            "missing_context": missing_context,
            "actions": actions
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_context_review", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_pipeline_context_review",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    // ── cascade.init ──
}
