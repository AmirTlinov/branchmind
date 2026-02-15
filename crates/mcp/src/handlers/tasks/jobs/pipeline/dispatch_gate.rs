#![forbid(unsafe_code)]

use super::*;
use std::collections::HashSet;

fn parse_jobs_artifact_ref(raw: &str, field: &str) -> Result<(String, String), Value> {
    let value = ensure_artifact_ref(raw, field)?;
    let Some(rest) = value.strip_prefix("artifact://jobs/") else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected artifact://jobs/JOB-.../<artifact_key> ref"),
        ));
    };
    let Some((job_id_raw, key_raw)) = rest.split_once('/') else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected artifact://jobs/JOB-.../<artifact_key> ref"),
        ));
    };
    let job_id = job_id_raw.trim();
    let key = key_raw.trim();
    if job_id.is_empty() || key.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: empty segment is not allowed"),
        ));
    }
    if !job_id.to_ascii_uppercase().starts_with("JOB-") {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: missing JOB-... segment"),
        ));
    }
    Ok((job_id.to_string(), key.to_string()))
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_gate(&mut self, args: Value) -> Value {
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
                "builder_batch_ref",
                "validator_report_ref",
                "policy",
            ],
            "jobs.pipeline.gate",
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
        let binding = match resolve_slice_binding_optional(self, &workspace, &slice_id) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if binding.is_none() && self.jobs_slice_first_fail_closed_enabled {
            return ai_error(
                "PRECONDITION_FAILED",
                "unknown slice_id: missing plan_slices binding (run tasks.slices.apply first)",
            );
        }
        if let Some(binding) = binding.as_ref()
            && task_id != binding.plan_id
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: task must match slice binding plan_id (slice-first)",
            );
        }
        let scout_pack_ref = match args_obj.get("scout_pack_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "scout_pack_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "scout_pack_ref is required"),
        };
        let builder_batch_ref = match args_obj.get("builder_batch_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "builder_batch_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "builder_batch_ref is required"),
        };
        let validator_report_ref = match args_obj
            .get("validator_report_ref")
            .and_then(|v| v.as_str())
        {
            Some(v) => match ensure_artifact_ref(v, "validator_report_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "validator_report_ref is required"),
        };
        let policy = match optional_non_empty_string(args_obj, "policy") {
            Ok(v) => v.unwrap_or_else(|| "fail_closed".to_string()),
            Err(resp) => return resp,
        };
        if !policy.eq_ignore_ascii_case("fail_closed") {
            return ai_error(
                "INVALID_INPUT",
                "policy must be fail_closed for jobs.pipeline.gate",
            );
        }

        let scout_job_id = match extract_job_id_from_ref(&scout_pack_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "scout_pack_ref must include a JOB-... lineage token",
                );
            }
        };
        let builder_job_id = match extract_job_id_from_ref(&builder_batch_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "builder_batch_ref must include a JOB-... lineage token",
                );
            }
        };
        let validator_job_id = match extract_job_id_from_ref(&validator_report_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "validator_report_ref must include a JOB-... lineage token",
                );
            }
        };
        if validator_job_id == builder_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "validator lineage must be independent from builder",
            );
        }

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
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown scout job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let builder_open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: builder_job_id.clone(),
                include_prompt: false,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown builder job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let validator_open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: validator_job_id.clone(),
                include_prompt: false,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => {
                return ai_error("UNKNOWN_ID", "Unknown validator job id");
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        if !scout_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: scout job is not DONE",
            );
        }
        if !builder_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: builder job is not DONE",
            );
        }
        if !validator_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: validator job is not DONE",
            );
        }

        let scout_summary = match scout_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.gate: scout job summary is empty",
                );
            }
        };
        let builder_summary = match builder_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.gate: builder job summary is empty",
                );
            }
        };
        let validator_summary = match validator_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.gate: validator job summary is empty",
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
        let builder_json = match parse_json_object_from_text(
            builder_summary,
            "builder summary (builder_diff_batch)",
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let validator_json = match parse_json_object_from_text(
            validator_summary,
            "validator summary (validator_report)",
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
        let (scout_norm, scout_warnings) = match validate_scout_context_pack_contract(
            &self.store,
            &workspace,
            &scout_json,
            scout_max_context_refs,
            &scout_policy,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let builder_norm = match validate_builder_diff_batch_contract(&builder_json) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let validator = match validate_validator_report_contract(&validator_json) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        // ── v2 enhanced validation (backward-compatible) ──

        // Try v2 validator report (intent compliance + traceability).
        let v2_report = crate::support::validate_validator_report_v2(&validator_json).ok();

        // Cross-validate writer output against scout scope if builder role is "writer".
        let builder_meta = parse_meta_map(builder_open.meta_json.as_deref());
        let validator_meta = parse_meta_map(validator_open.meta_json.as_deref());
        let builder_role = builder_meta
            .get("pipeline_role")
            .and_then(|v| v.as_str())
            .unwrap_or("builder");
        let builder_context_retry_count = builder_meta
            .get("context_retry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let builder_context_retry_limit = builder_meta
            .get("context_retry_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(MAX_CONTEXT_RETRY_LIMIT)
            .min(MAX_CONTEXT_RETRY_LIMIT);
        let builder_context_request = builder_norm.get("context_request").cloned();
        let builder_requested_context = builder_context_request.is_some();

        // Enforce slice budgets (fail-closed) for builder batches with actual diffs.
        if self.slice_budgets_enforced_enabled && !builder_requested_context {
            let budgets = match binding.as_ref() {
                Some(binding) => binding.spec.budgets.clone(),
                None => match crate::support::SliceBudgets::parse(
                    builder_meta.get("slice_budgets"),
                    "slice_budgets",
                ) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                },
            };
            let max_files = budgets.max_files;
            let max_diff_lines = budgets.max_diff_lines;
            let changes = builder_norm
                .get("changes")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut unique_paths = HashSet::<String>::new();
            let mut unique_artifacts = HashSet::<String>::new();
            let mut total_lines = 0usize;
            for (idx, change) in changes.iter().enumerate() {
                let Some(obj) = change.as_object() else {
                    continue;
                };
                if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                    unique_paths.insert(path.trim().to_string());
                }
                let Some(diff_ref) = obj.get("diff_ref").and_then(|v| v.as_str()) else {
                    return ai_error(
                        "PRECONDITION_FAILED",
                        &format!(
                            "builder_diff_batch.changes[{idx}].diff_ref missing (cannot enforce budgets)"
                        ),
                    );
                };
                let (job_id, key) = match parse_jobs_artifact_ref(
                    diff_ref,
                    "builder_diff_batch.changes[].diff_ref",
                ) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                if job_id != builder_job_id {
                    return ai_error(
                        "PRECONDITION_FAILED",
                        "builder diff_ref must point to artifacts under the same builder job_id",
                    );
                }
                if unique_artifacts.insert(key.clone()) {
                    let artifact = match self.store.job_artifact_get(
                        &workspace,
                        bm_storage::JobArtifactGetRequest {
                            job_id: job_id.clone(),
                            artifact_key: key.clone(),
                        },
                    ) {
                        Ok(v) => v,
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    let Some(artifact) = artifact else {
                        return ai_error(
                            "PRECONDITION_FAILED",
                            &format!("missing job artifact for diff_ref: {diff_ref}"),
                        );
                    };
                    total_lines = total_lines.saturating_add(artifact.content_text.lines().count());
                }
            }
            if unique_paths.len() > max_files {
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "slice budgets violated: max_files={} but builder changes touched {} files",
                        max_files,
                        unique_paths.len()
                    ),
                );
            }
            if total_lines > max_diff_lines {
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "slice budgets violated: max_diff_lines={} but diff artifacts have {} lines",
                        max_diff_lines, total_lines
                    ),
                );
            }
        }

        let builder_meta_scout_job = builder_meta
            .get("scout_job_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if builder_meta_scout_job.is_empty() || builder_meta_scout_job != scout_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: builder lineage mismatch for scout_job_id",
            );
        }
        let validator_meta_scout_job = validator_meta
            .get("scout_job_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if validator_meta_scout_job.is_empty() || validator_meta_scout_job != scout_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: validator lineage mismatch for scout_job_id",
            );
        }
        let validator_meta_builder_job = validator_meta
            .get("builder_job_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if validator_meta_builder_job.is_empty() || validator_meta_builder_job != builder_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.gate: validator lineage mismatch for builder_job_id",
            );
        }
        // Validate writer patch pack if builder role is "writer".
        if builder_role == "writer"
            && let Err(resp) = crate::support::validate_writer_patch_pack(&builder_json)
        {
            return resp;
        }

        let mut cross_validation_violations = Vec::<String>::new();
        if builder_role == "writer" {
            let writer_affected = builder_norm
                .get("affected_files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let scout_scope_in = scout_norm
                .get("scope")
                .and_then(|v| v.get("in"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let scout_change_paths = scout_norm
                .get("change_hints")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            v.get("path")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            cross_validation_violations = crate::support::cross_validate_writer_scout(
                &writer_affected,
                &scout_scope_in,
                &scout_change_paths,
            );
        }

        if builder_norm
            .get("slice_id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != slice_id)
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "builder_diff_batch.slice_id does not match gate slice_id",
            );
        }
        let builder_evidence_revision = builder_norm
            .get("execution_evidence")
            .and_then(|v| v.get("revision"))
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        if builder_evidence_revision != builder_open.job.revision {
            return ai_error(
                "PRECONDITION_FAILED",
                "builder_diff_batch.execution_evidence.revision must match builder revision at gate",
            );
        }
        if validator
            .normalized
            .get("slice_id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != slice_id)
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "validator_report.slice_id does not match gate slice_id",
            );
        }

        let mut decision = match validator.recommendation.as_str() {
            "approve" => "approve".to_string(),
            "rework" => "rework".to_string(),
            _ => "reject".to_string(),
        };
        let mut reasons = Vec::<String>::new();
        reasons.push(format!(
            "validator recommendation={}",
            validator.recommendation
        ));
        if builder_requested_context {
            let retry_budget_available = builder_context_retry_count < builder_context_retry_limit;
            if retry_budget_available {
                decision = "rework".to_string();
                reasons.push(format!(
                    "builder requested additional scout context (retry {}/{})",
                    builder_context_retry_count + 1,
                    builder_context_retry_limit
                ));
            } else {
                decision = "reject".to_string();
                reasons.push(format!(
                    "builder context request retry budget exhausted ({}/{})",
                    builder_context_retry_count, builder_context_retry_limit
                ));
            }
        }
        if decision != "approve"
            && let Some(actions) = validator
                .normalized
                .get("rework_actions")
                .and_then(|v| v.as_array())
        {
            for item in actions.iter().filter_map(|v| v.as_str()) {
                let item = item.trim();
                if !item.is_empty() {
                    reasons.push(format!("rework: {item}"));
                }
            }
        }

        let decision_ref = build_decision_ref(
            &task_id,
            &slice_id,
            &decision,
            &builder_job_id,
            &validator_job_id,
            builder_open.job.revision,
        );
        let mut actions = Vec::<Value>::new();
        if decision == "approve" {
            actions.push(json!({
                "cmd": "jobs.pipeline.apply",
                "args": {
                    "task": task_id,
                    "slice_id": slice_id,
                    "decision_ref": decision_ref,
                    "builder_batch_ref": builder_batch_ref,
                    "expected_revision": builder_open.job.revision
                }
            }));
        } else if builder_requested_context
            && builder_context_retry_count < builder_context_retry_limit
        {
            let context_reason = builder_context_request
                .as_ref()
                .and_then(|v| v.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("builder requested additional context");
            let missing_context = builder_context_request
                .as_ref()
                .and_then(|v| v.get("missing_context"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut constraints_payload = missing_context.clone();
            constraints_payload.push(Value::String(format!("context_reason: {context_reason}")));
            let scout_anchor = scout_meta
                .get("anchor")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    scout_meta
                        .get("pipeline")
                        .and_then(|v| v.get("anchor"))
                        .and_then(|v| v.as_str())
                })
                .map(str::to_string);
            if let Some(anchor) = scout_anchor {
                actions.push(json!({
                    "cmd": "jobs.macro.dispatch.scout",
                    "args": {
                        "task": task_id,
                        "anchor": anchor,
                        "slice_id": slice_id,
                        "constraints": constraints_payload,
                        "quality_profile": "flagship",
                        "meta": {
                            "requested_by": "builder_context_request",
                            "context_retry_count": builder_context_retry_count + 1,
                            "context_retry_limit": builder_context_retry_limit,
                            "source_builder_job_id": builder_job_id
                        }
                    }
                }));
            } else {
                actions.push(json!({
                    "cmd": "jobs.macro.dispatch.builder",
                    "args": {
                        "task": task_id,
                        "slice_id": slice_id,
                        "scout_pack_ref": scout_pack_ref,
                        "context_retry_count": builder_context_retry_count + 1,
                        "context_retry_limit": builder_context_retry_limit
                    }
                }));
            }
        } else {
            actions.push(json!({
                "cmd": "jobs.macro.dispatch.builder",
                "args": {
                    "task": task_id,
                    "slice_id": slice_id,
                    "scout_pack_ref": scout_pack_ref,
                    "context_retry_count": builder_context_retry_count,
                    "context_retry_limit": builder_context_retry_limit
                }
            }));
        }

        let _ = publish_optional_mesh_message(
            self,
            &workspace,
            MeshMessageRequest {
                task_id: Some(task_id.clone()),
                from_agent_id: self.default_agent_id.clone(),
                thread_id: None,
                idempotency_key: Some(format!(
                    "jobs.pipeline.transition:{}:{}:{}",
                    task_id, slice_id, scout_job_id
                )),
                kind: "scout_ready".to_string(),
                summary: "scout artifact validated".to_string(),
                payload: json!({"task": task_id, "slice_id": slice_id, "job_id": scout_job_id}),
            },
        );
        let _ = publish_optional_mesh_message(
            self,
            &workspace,
            MeshMessageRequest {
                task_id: Some(task_id.clone()),
                from_agent_id: self.default_agent_id.clone(),
                thread_id: None,
                idempotency_key: Some(format!(
                    "jobs.pipeline.transition:{}:{}:{}",
                    task_id, slice_id, builder_job_id
                )),
                kind: "builder_ready".to_string(),
                summary: "builder artifact validated".to_string(),
                payload: json!({"task": task_id, "slice_id": slice_id, "job_id": builder_job_id}),
            },
        );
        let _ = publish_optional_mesh_message(
            self,
            &workspace,
            MeshMessageRequest {
                task_id: Some(task_id.clone()),
                from_agent_id: self.default_agent_id.clone(),
                thread_id: None,
                idempotency_key: Some(format!(
                    "jobs.pipeline.transition:{}:{}:{}",
                    task_id, slice_id, validator_job_id
                )),
                kind: "validator_ready".to_string(),
                summary: "validator artifact validated".to_string(),
                payload: json!({"task": task_id, "slice_id": slice_id, "job_id": validator_job_id}),
            },
        );
        let gate_mesh = match publish_optional_mesh_message(
            self,
            &workspace,
            MeshMessageRequest {
                task_id: Some(task_id.clone()),
                from_agent_id: self.default_agent_id.clone(),
                thread_id: None,
                idempotency_key: Some(format!(
                    "jobs.pipeline.gate:{}:{}:{}",
                    task_id, slice_id, decision
                )),
                kind: "gate_decision".to_string(),
                summary: format!("lead decision={decision}"),
                payload: json!({
                "task": task_id,
                "slice_id": slice_id,
                "decision": decision,
                "decision_ref": decision_ref,
                "builder_job_id": builder_open.job.id,
                "validator_job_id": validator_open.job.id
                }),
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        warnings.extend(scout_warnings);

        // Add cross-validation violations as warnings.
        for violation in &cross_validation_violations {
            warnings.push(json!({
                "code": "CROSS_VALIDATION",
                "message": violation
            }));
        }

        // Build v2 enhanced gate data (if available).
        let v2_gate = match &v2_report {
            Some(v2) => json!({
                "intent_compliance": v2.normalized.get("intent_compliance"),
                "traceability": v2.normalized.get("traceability"),
                "security_findings": v2.normalized.get("security_findings"),
                "cross_validation_violations": cross_validation_violations
            }),
            None => Value::Null,
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "task": task_id,
            "slice_id": slice_id,
            "policy": "fail_closed",
            "decision": decision,
            "decision_ref": decision_ref,
            "reasons": reasons,
            "actions": actions,
            "lineage": {
                "scout_job_id": scout_open.job.id,
                "builder_job_id": builder_open.job.id,
                "validator_job_id": validator_open.job.id,
                "validator_independent": validator_open.job.id != builder_open.job.id
            },
            "context_loop": {
                "builder_requested_context": builder_requested_context,
                "context_retry_count": builder_context_retry_count,
                "context_retry_limit": builder_context_retry_limit
            },
            "normalized": {
                "scout_context_pack": scout_norm,
                "builder_diff_batch": builder_norm,
                "validator_report": validator.normalized
            },
            "v2_gate": v2_gate,
            "mesh": gate_mesh
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_gate", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_pipeline_gate", result, warnings, Vec::new())
        }
    }
}
