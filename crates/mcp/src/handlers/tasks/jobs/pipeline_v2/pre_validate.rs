#![forbid(unsafe_code)]

use super::super::*;
use super::{parse_meta_map, scout_policy_from_meta};
use crate::support::{
    ensure_artifact_ref, extract_job_id_from_ref, parse_json_object_from_text,
    pre_validate_scout_pack, validate_scout_context_pack as validate_scout_context_pack_contract,
};
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_pre_validate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &["workspace", "task", "slice_id", "scout_pack_ref"],
            "jobs.pipeline.pre_validate",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
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

        // Open scout job and parse pack.
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
                "jobs.pipeline.pre_validate: scout job is not DONE",
            );
        }
        let scout_summary = match scout_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.pre_validate: scout job summary is empty",
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
                    "scout_pack_ref": scout_pack_ref,
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
                    }
                });
                return if warnings.is_empty() {
                    ai_ok("tasks_jobs_pipeline_pre_validate", result)
                } else {
                    ai_ok_with_warnings(
                        "tasks_jobs_pipeline_pre_validate",
                        result,
                        warnings,
                        Vec::new(),
                    )
                };
            }
        };
        warnings.extend(scout_contract_warnings);

        // Extract anchors from scout pack (lightweight parse, no store validation).
        use crate::support::{ScoutAnchorType, ScoutAnchorV2};
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

        // Run deterministic pre-validator.
        let (verdict, checks) = pre_validate_scout_pack(&scout_norm, &anchors_v2);

        let verdict_json = match &verdict {
            crate::support::PreValidatorVerdict::Pass => json!({"status": "pass"}),
            crate::support::PreValidatorVerdict::NeedMore { hints } => {
                json!({"status": "need_more", "hints": hints})
            }
            crate::support::PreValidatorVerdict::Reject { reason } => {
                json!({"status": "reject", "reason": reason})
            }
        };
        let checks_json = json!({
            "completeness_ok": checks.completeness_ok,
            "completeness_missing": checks.completeness_missing,
            "dependencies_ok": checks.dependencies_ok,
            "dependencies_missing": checks.dependencies_missing,
            "patterns_ok": checks.patterns_ok,
            "patterns_missing": checks.patterns_missing,
            "intent_coverage_ok": checks.intent_coverage_ok,
            "intent_coverage_missing": checks.intent_coverage_missing
        });

        let result = json!({
            "workspace": workspace.as_str(),
            "scout_pack_ref": scout_pack_ref,
            "verdict": verdict_json,
            "checks": checks_json
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_pre_validate", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_pipeline_pre_validate",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    // ── pipeline.context.review (deterministic + optional cheap semantic score) ──
}
