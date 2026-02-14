#![forbid(unsafe_code)]

use super::*;
use crate::support::{
    SliceBudgets, ensure_artifact_ref, extract_job_id_from_ref, parse_json_object_from_text,
    validate_builder_diff_batch as validate_builder_diff_batch_contract,
};
use serde_json::{Value, json};
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
    Ok((job_id.to_string(), key.to_string()))
}

#[derive(Clone, Debug)]
struct DecisionRef {
    task: String,
    slice_id: String,
    decision: String,
    builder_job_id: String,
    validator_job_id: String,
    builder_revision: i64,
}

fn parse_decision_ref(raw: &str) -> Result<DecisionRef, Value> {
    let value = ensure_artifact_ref(raw, "decision_ref")?;

    let Some(rest) = value.strip_prefix("artifact://pipeline/gate/") else {
        return Err(ai_error(
            "INVALID_INPUT",
            "decision_ref: expected artifact://pipeline/gate/... ref",
        ));
    };
    let parts = rest.split('/').collect::<Vec<_>>();
    if parts.len() != 9 || parts[3] != "builder" || parts[5] != "validator" || parts[7] != "rev" {
        return Err(ai_error(
            "INVALID_INPUT",
            "decision_ref: unsupported format",
        ));
    }
    let task = parts[0].trim();
    let slice_id = parts[1].trim();
    let decision = parts[2].trim().to_ascii_lowercase();
    let builder_job_id = parts[4].trim();
    let validator_job_id = parts[6].trim();
    let builder_revision = parts[8]
        .trim()
        .parse::<i64>()
        .map_err(|_| ai_error("INVALID_INPUT", "decision_ref: invalid builder revision"))?;
    if task.is_empty()
        || slice_id.is_empty()
        || decision.is_empty()
        || builder_job_id.is_empty()
        || validator_job_id.is_empty()
    {
        return Err(ai_error(
            "INVALID_INPUT",
            "decision_ref: empty segment is not allowed",
        ));
    }
    Ok(DecisionRef {
        task: task.to_string(),
        slice_id: slice_id.to_string(),
        decision,
        builder_job_id: builder_job_id.to_string(),
        validator_job_id: validator_job_id.to_string(),
        builder_revision,
    })
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_pipeline_apply(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "slice_id",
                "decision_ref",
                "builder_batch_ref",
                "expected_revision",
            ],
            "jobs.pipeline.apply",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task_id = match super::pipeline::require_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let slice_id = match super::pipeline::require_non_empty_string(args_obj, "slice_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let binding =
            match super::pipeline::resolve_slice_binding_optional(self, &workspace, &slice_id) {
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
                "jobs.pipeline.apply: task must match slice binding plan_id (slice-first)",
            );
        }
        let decision_ref_raw = match args_obj.get("decision_ref").and_then(|v| v.as_str()) {
            Some(v) => v,
            None => return ai_error("INVALID_INPUT", "decision_ref is required"),
        };
        let decision_ref = match parse_decision_ref(decision_ref_raw) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let builder_batch_ref = match args_obj.get("builder_batch_ref").and_then(|v| v.as_str()) {
            Some(v) => match ensure_artifact_ref(v, "builder_batch_ref") {
                Ok(s) => s,
                Err(resp) => return resp,
            },
            None => return ai_error("INVALID_INPUT", "builder_batch_ref is required"),
        };
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("INVALID_INPUT", "expected_revision is required"),
            Err(resp) => return resp,
        };

        if decision_ref.task != task_id || decision_ref.slice_id != slice_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "decision_ref does not match task/slice_id",
            );
        }
        if decision_ref.decision != "approve" {
            return ai_error_with(
                "PRECONDITION_FAILED",
                "jobs.pipeline.apply requires decision=approve",
                Some("Run jobs.pipeline.gate and resolve rework/reject reasons first."),
                Vec::new(),
            );
        }
        let builder_job_id = match extract_job_id_from_ref(&builder_batch_ref) {
            Some(v) => v,
            None => {
                return ai_error(
                    "INVALID_INPUT",
                    "builder_batch_ref must include a JOB-... lineage token",
                );
            }
        };
        if decision_ref.builder_job_id != builder_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "decision_ref builder lineage does not match builder_batch_ref",
            );
        }

        if decision_ref.validator_job_id == builder_job_id {
            return ai_error(
                "PRECONDITION_FAILED",
                "validator lineage invalid: validator must be independent from builder",
            );
        }

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
        if !builder_open.job.status.eq_ignore_ascii_case("DONE") {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.apply: builder job must be DONE",
            );
        }
        if builder_open.job.revision != expected_revision {
            return ai_error_with(
                "REVISION_MISMATCH",
                "builder revision changed since gate decision",
                Some("Refresh jobs.pipeline.gate and retry with the new expected_revision."),
                vec![json!({
                    "expected_revision": expected_revision,
                    "actual_revision": builder_open.job.revision
                })],
            );
        }
        if decision_ref.builder_revision != builder_open.job.revision {
            return ai_error_with(
                "REVISION_MISMATCH",
                "decision_ref revision does not match current builder revision",
                Some("Re-run jobs.pipeline.gate to produce a fresh decision_ref."),
                vec![json!({
                    "decision_revision": decision_ref.builder_revision,
                    "actual_revision": builder_open.job.revision
                })],
            );
        }

        let builder_summary = match builder_open.job.summary.as_deref() {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                return ai_error(
                    "PRECONDITION_FAILED",
                    "jobs.pipeline.apply: builder summary is empty",
                );
            }
        };
        let builder_json = match parse_json_object_from_text(
            builder_summary,
            "builder summary (builder_diff_batch)",
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let builder_norm = match validate_builder_diff_batch_contract(&builder_json) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if builder_norm
            .get("slice_id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != slice_id)
        {
            return ai_error(
                "PRECONDITION_FAILED",
                "builder_diff_batch.slice_id does not match apply slice_id",
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
                "builder_diff_batch.execution_evidence.revision must match current builder revision",
            );
        }

        // Validate diff artifacts (always), then enforce slice budgets (defense in depth).
        let changes = builder_norm
            .get("changes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if changes.is_empty() {
            return ai_error(
                "PRECONDITION_FAILED",
                "jobs.pipeline.apply: builder_diff_batch.changes must not be empty",
            );
        }
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
                    &format!("builder_diff_batch.changes[{idx}].diff_ref is required"),
                );
            };
            let field = format!("builder_diff_batch.changes[{idx}].diff_ref");
            let (job_id, key) = match parse_jobs_artifact_ref(diff_ref, &field) {
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
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
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
        if self.slice_budgets_enforced_enabled {
            let builder_meta = super::pipeline::parse_meta_map(builder_open.meta_json.as_deref());
            let budgets = match binding.as_ref() {
                Some(binding) => binding.spec.budgets.clone(),
                None => {
                    match SliceBudgets::parse(builder_meta.get("slice_budgets"), "slice_budgets") {
                        Ok(v) => v,
                        Err(resp) => return resp,
                    }
                }
            };
            if unique_paths.len() > budgets.max_files {
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "slice budgets violated: max_files={} but builder changes touched {} files",
                        budgets.max_files,
                        unique_paths.len()
                    ),
                );
            }
            if total_lines > budgets.max_diff_lines {
                return ai_error(
                    "PRECONDITION_FAILED",
                    &format!(
                        "slice budgets violated: max_diff_lines={} but diff artifacts have {} lines",
                        budgets.max_diff_lines, total_lines
                    ),
                );
            }
        }

        let mesh = match super::pipeline::publish_optional_mesh_message(
            self,
            &workspace,
            super::pipeline::MeshMessageRequest {
                task_id: Some(task_id.clone()),
                from_agent_id: self.default_agent_id.clone(),
                thread_id: None,
                idempotency_key: Some(format!(
                    "jobs.pipeline.apply:{}:{}:{}",
                    task_id, slice_id, builder_job_id
                )),
                kind: "pipeline_apply".to_string(),
                summary: "lead approved batch applied".to_string(),
                payload: json!({
                "task": task_id,
                "slice_id": slice_id,
                "decision_ref": decision_ref_raw,
                "builder_batch_ref": builder_batch_ref,
                "expected_revision": expected_revision
                }),
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let result = json!({
            "workspace": workspace.as_str(),
            "task": task_id,
            "slice_id": slice_id,
            "status": "applied",
            "decision_ref": decision_ref_raw,
            "applied_revision": builder_open.job.revision,
            "builder_batch_ref": builder_batch_ref,
            "applied_batch": builder_norm,
            "mesh": mesh
        });
        if warnings.is_empty() {
            ai_ok("tasks_jobs_pipeline_apply", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_pipeline_apply", result, warnings, Vec::new())
        }
    }
}
