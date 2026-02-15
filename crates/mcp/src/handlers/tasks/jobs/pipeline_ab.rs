#![forbid(unsafe_code)]

use super::*;
use crate::support::{
    ensure_artifact_ref, extract_job_id_from_ref, parse_json_object_from_text,
    validate_validator_report as validate_validator_report_contract,
};
use serde_json::{Value, json};

const DEFAULT_SCOUT_MODEL: &str = "haiku";
const DEFAULT_EXECUTOR_PROFILE: &str = "xhigh";

fn require_non_empty_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Value> {
    let raw = require_string(args_obj, key)?;
    let v = raw.trim();
    if v.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(v.to_string())
}

fn optional_non_empty_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    let Some(v) = optional_string(args_obj, key)? else {
        return Ok(None);
    };
    let v = v.trim();
    if v.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(Some(v.to_string()))
}

fn normalize_string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, Value> {
    let Some(raw) = value else {
        return Ok(Vec::new());
    };
    let arr = raw
        .as_array()
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field} must be an array")))?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let Some(s) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field}[{idx}] must be a string"),
            ));
        };
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn parse_scout_mode_from_variant(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
    default_mode: &str,
) -> Result<String, Value> {
    let Some(value) = args_obj.get(key) else {
        return Ok(default_mode.to_string());
    };
    let obj = value
        .as_object()
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{key} must be an object")))?;
    let mode = obj
        .get("scout_mode")
        .and_then(|v| v.as_str())
        .unwrap_or(default_mode)
        .trim()
        .to_ascii_lowercase();
    if !matches!(mode.as_str(), "weak" | "strong") {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key}.scout_mode must be weak|strong"),
        ));
    }
    Ok(mode)
}

fn ab_variant_profile(mode: &str) -> (String, String, bool, f64, f64, Value) {
    if mode.eq_ignore_ascii_case("weak") {
        (
            "standard".to_string(),
            "warn".to_string(),
            false,
            0.80,
            0.70,
            json!({
                "require_objective_coverage": false,
                "require_dod_coverage": false,
                "require_test_hints": 2,
                "require_risk_falsifier_pairs": 2
            }),
        )
    } else {
        (
            "flagship".to_string(),
            "strict".to_string(),
            true,
            0.35,
            0.25,
            json!({
                "require_objective_coverage": true,
                "require_dod_coverage": true,
                "require_test_hints": 3,
                "require_risk_falsifier_pairs": 3
            }),
        )
    }
}

fn open_validator_report_contract(
    server: &mut McpServer,
    workspace: &bm_core::ids::WorkspaceId,
    report_ref: &str,
) -> Result<crate::support::ValidatorReportContract, Value> {
    let job_id = extract_job_id_from_ref(report_ref).ok_or_else(|| {
        ai_error(
            "INVALID_INPUT",
            "validator_report_ref must include a JOB-... lineage token",
        )
    })?;
    let opened = match server.store.job_open(
        workspace,
        bm_storage::JobOpenRequest {
            id: job_id,
            include_prompt: false,
            include_events: false,
            include_meta: false,
            max_events: 0,
            before_seq: None,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => {
            return Err(ai_error("UNKNOWN_ID", "Unknown validator job id"));
        }
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    if !opened.job.status.eq_ignore_ascii_case("DONE") {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "validator_report job must be DONE for A/B comparison",
        ));
    }
    let summary = opened
        .job
        .summary
        .as_deref()
        .ok_or_else(|| ai_error("PRECONDITION_FAILED", "validator_report summary is empty"))?;
    let summary_json =
        parse_json_object_from_text(summary, "validator summary (validator_report)")?;
    validate_validator_report_contract(&summary_json)
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_ab_slice(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "anchor",
                "slice_id",
                "objective",
                "constraints",
                "variant_a",
                "variant_b",
                "policy",
                "dry_run",
                "validator_report_ref_a",
                "validator_report_ref_b",
            ],
            "jobs.pipeline.ab.slice",
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
        let anchor_id = match require_non_empty_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match require_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let objective = match require_non_empty_string(args_obj, "objective") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let constraints = match normalize_string_array(args_obj.get("constraints"), "constraints") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let policy = match optional_non_empty_string(args_obj, "policy") {
            Ok(v) => v.unwrap_or_else(|| "fail_closed".to_string()),
            Err(resp) => return resp,
        };
        if !policy.eq_ignore_ascii_case("fail_closed") {
            return ai_error(
                "INVALID_INPUT",
                "policy must be fail_closed for jobs.pipeline.ab.slice",
            );
        }
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let mode_a = match parse_scout_mode_from_variant(args_obj, "variant_a", "weak") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mode_b = match parse_scout_mode_from_variant(args_obj, "variant_b", "strong") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let make_dispatch_args =
            |variant_label: &str, mode: &str, variant_slice_id: String| -> Value {
                let (
                    quality_profile,
                    novelty_policy,
                    critic_pass,
                    max_anchor_overlap,
                    max_ref_redundancy,
                    coverage_targets,
                ) = ab_variant_profile(mode);
                json!({
                    "workspace": workspace.as_str(),
                    "task": task_id,
                    "anchor": anchor_id,
                    "slice_id": variant_slice_id,
                    "objective": format!("[AB:{variant_label}:{mode}] {objective}"),
                    "constraints": constraints,
                    "model": DEFAULT_SCOUT_MODEL,
                    "quality_profile": quality_profile,
                    "novelty_policy": novelty_policy,
                    "critic_pass": critic_pass,
                    "coverage_targets": coverage_targets,
                    "max_anchor_overlap": max_anchor_overlap,
                    "max_ref_redundancy": max_ref_redundancy,
                    "dry_run": dry_run,
                })
            };

        // Slice-first: keep the canonical slice_id so dispatch.scout can resolve the plan_slices binding.
        // Variants are differentiated by the objective prefix and returned metadata, not by mutating slice_id.
        let slice_a = slice_id.clone();
        let slice_b = slice_id.clone();
        let dispatch_args_a = make_dispatch_args("A", &mode_a, slice_a.clone());
        let dispatch_args_b = make_dispatch_args("B", &mode_b, slice_b.clone());
        let mut run_a = json!({
            "variant": "A",
            "mode": mode_a.clone(),
            "slice_id": slice_a,
            "dispatch": Value::Null
        });
        let mut run_b = json!({
            "variant": "B",
            "mode": mode_b.clone(),
            "slice_id": slice_b,
            "dispatch": Value::Null
        });

        if !dry_run {
            let dispatch_a = self.tool_tasks_jobs_macro_dispatch_scout(dispatch_args_a.clone());
            if dispatch_a
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(obj) = run_a.as_object_mut() {
                    obj.insert(
                        "dispatch".to_string(),
                        dispatch_a.get("result").cloned().unwrap_or(Value::Null),
                    );
                }
            } else {
                return dispatch_a;
            }

            let dispatch_b = self.tool_tasks_jobs_macro_dispatch_scout(dispatch_args_b.clone());
            if dispatch_b
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(obj) = run_b.as_object_mut() {
                    obj.insert(
                        "dispatch".to_string(),
                        dispatch_b.get("result").cloned().unwrap_or(Value::Null),
                    );
                }
            } else {
                return dispatch_b;
            }
        }

        let validator_ref_a = match optional_non_empty_string(args_obj, "validator_report_ref_a") {
            Ok(v) => v.map(|raw| ensure_artifact_ref(&raw, "validator_report_ref_a")),
            Err(resp) => return resp,
        };
        let validator_ref_a = match validator_ref_a {
            Some(Ok(v)) => Some(v),
            Some(Err(resp)) => return resp,
            None => None,
        };
        let validator_ref_b = match optional_non_empty_string(args_obj, "validator_report_ref_b") {
            Ok(v) => v.map(|raw| ensure_artifact_ref(&raw, "validator_report_ref_b")),
            Err(resp) => return resp,
        };
        let validator_ref_b = match validator_ref_b {
            Some(Ok(v)) => Some(v),
            Some(Err(resp)) => return resp,
            None => None,
        };

        let mut metrics = json!({
            "plan_fit_score": { "a": Value::Null, "b": Value::Null, "delta": Value::Null },
            "rework_actions": { "a": Value::Null, "b": Value::Null, "delta": Value::Null },
            "reopen_rate": { "a": Value::Null, "b": Value::Null, "delta": Value::Null }
        });
        let mut decision = "inconclusive".to_string();
        let mut reasons =
            vec!["A/B dispatch configured with fail-closed scout quality contracts.".to_string()];

        if let (Some(report_ref_a), Some(report_ref_b)) = (validator_ref_a, validator_ref_b) {
            let validator_a = match open_validator_report_contract(self, &workspace, &report_ref_a)
            {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let validator_b = match open_validator_report_contract(self, &workspace, &report_ref_b)
            {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let plan_fit_a = validator_a
                .normalized
                .get("plan_fit_score")
                .and_then(|v| v.as_i64())
                .unwrap_or_default();
            let plan_fit_b = validator_b
                .normalized
                .get("plan_fit_score")
                .and_then(|v| v.as_i64())
                .unwrap_or_default();
            let rework_count_a = validator_a
                .normalized
                .get("rework_actions")
                .and_then(|v| v.as_array())
                .map(|v| v.len() as i64)
                .unwrap_or_default();
            let rework_count_b = validator_b
                .normalized
                .get("rework_actions")
                .and_then(|v| v.as_array())
                .map(|v| v.len() as i64)
                .unwrap_or_default();
            let reopen_a = if validator_a.recommendation == "approve" {
                0.0
            } else {
                1.0
            };
            let reopen_b = if validator_b.recommendation == "approve" {
                0.0
            } else {
                1.0
            };
            metrics = json!({
                "plan_fit_score": { "a": plan_fit_a, "b": plan_fit_b, "delta": plan_fit_b - plan_fit_a },
                "rework_actions": { "a": rework_count_a, "b": rework_count_b, "delta": rework_count_b - rework_count_a },
                "reopen_rate": { "a": reopen_a, "b": reopen_b, "delta": reopen_b - reopen_a }
            });

            decision = if reopen_b < reopen_a
                || (reopen_b == reopen_a
                    && (plan_fit_b > plan_fit_a
                        || (plan_fit_b == plan_fit_a && rework_count_b < rework_count_a)))
            {
                "prefer_b".to_string()
            } else if reopen_a < reopen_b
                || (reopen_a == reopen_b
                    && (plan_fit_a > plan_fit_b
                        || (plan_fit_a == plan_fit_b && rework_count_a < rework_count_b)))
            {
                "prefer_a".to_string()
            } else {
                "inconclusive".to_string()
            };
            reasons.push(format!(
                "Compared validator reports: plan_fit A={plan_fit_a}, B={plan_fit_b}; rework_actions A={rework_count_a}, B={rework_count_b}."
            ));
            reasons.push(format!(
                "Recommendations: A={}, B={}.",
                validator_a.recommendation, validator_b.recommendation
            ));
        } else {
            reasons.push(
                "Provide validator_report_ref_a + validator_report_ref_b to compute final A/B metrics and preference."
                    .to_string(),
            );
        }

        let actions = vec![
            json!({
                "op": "call",
                "cmd": "jobs.macro.dispatch.builder",
                "reason": "Continue A-variant after scout completion.",
                "priority": "high",
                "budget_profile": "portal",
                "args": {
                    "task": task_id,
                    "slice_id": slice_id,
                    "scout_pack_ref": "artifact://jobs/<A-scout-job>/scout_context_pack",
                    "objective": format!("[AB:A:{mode_a}] {objective}"),
                    "dod": {"criteria": [], "tests": [], "security": []},
                    "executor": "auto",
                    "executor_profile": DEFAULT_EXECUTOR_PROFILE
                }
            }),
            json!({
                "op": "call",
                "cmd": "jobs.macro.dispatch.builder",
                "reason": "Continue B-variant after scout completion.",
                "priority": "high",
                "budget_profile": "portal",
                "args": {
                    "task": task_id,
                    "slice_id": slice_id,
                    "scout_pack_ref": "artifact://jobs/<B-scout-job>/scout_context_pack",
                    "objective": format!("[AB:B:{mode_b}] {objective}"),
                    "dod": {"criteria": [], "tests": [], "security": []},
                    "executor": "auto",
                    "executor_profile": DEFAULT_EXECUTOR_PROFILE
                }
            }),
        ];

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        let result = json!({
            "workspace": workspace.as_str(),
            "task": task_id,
            "anchor": anchor_id,
            "slice_id": slice_id,
            "policy": "fail_closed",
            "dry_run": dry_run,
            "runs": { "a": run_a, "b": run_b },
            "metrics": metrics,
            "decision": decision,
            "reasons": reasons,
            "actions": actions,
            "variants": {
                "a": dispatch_args_a,
                "b": dispatch_args_b
            }
        });
        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_ab_slice", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_pipeline_ab_slice", result, warnings, Vec::new())
        }
    }
}
