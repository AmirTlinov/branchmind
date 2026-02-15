#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_complete(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "job",
            "runner_id",
            "claim_revision",
            "status",
            "summary",
            "refs",
            "meta",
        ],
        "jobs.complete",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let workspace = match require_workspace(args_obj) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    let job_id = match require_string(args_obj, "job") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let runner_id = match require_string(args_obj, "runner_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let claim_revision = match optional_i64(args_obj, "claim_revision") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(claim_revision) = claim_revision else {
        return ai_error("INVALID_INPUT", "claim_revision is required");
    };
    let status = match require_string(args_obj, "status") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let summary = match optional_string(args_obj, "summary") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let mut refs = match optional_string_array(args_obj, "refs") {
        Ok(v) => v.unwrap_or_else(Vec::new),
        Err(resp) => return resp,
    };
    let status_norm = status.trim().to_ascii_uppercase();
    let mut completion_warnings = Vec::<Value>::new();
    let mut canonical_summary = summary.clone();

    // Proof-first DX: if refs were forgotten but proof is present in summary text,
    // salvage stable references deterministically to avoid needless proof-gate loops.
    if status_norm == "DONE"
        && let Some(s) = summary.as_deref()
        && !s.trim().is_empty()
    {
        refs = crate::salvage_job_completion_refs(s, &job_id, &refs);
    }

    // Artifact-first DX: if a job expects artifacts, materialize them into job_artifacts on DONE.
    // This keeps artifact://jobs/JOB-*/key refs real and readable (no “pretend refs”).
    if status_norm == "DONE" {
        let open = match server.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: job_id.clone(),
                include_prompt: false,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        // Defensive: don't write artifacts unless the caller holds the current lease (same checks
        // as job_complete, but earlier to avoid storing artifacts on claim mismatch).
        if !open.job.status.eq_ignore_ascii_case("RUNNING") {
            return ai_error(
                "CONFLICT",
                "jobs.complete: job is not RUNNING (cannot materialize artifacts)",
            );
        }
        let runner_match = open.job.runner.as_deref() == Some(runner_id.as_str());
        if open.job.revision != claim_revision || !runner_match {
            return ai_error(
                "CONFLICT",
                "jobs.complete: job claim mismatch (cannot materialize artifacts)",
            );
        }

        let expected_artifacts =
            crate::support::expected_artifacts_from_meta_json(open.meta_json.as_deref());

        if !expected_artifacts.is_empty() {
            if expected_artifacts.len() != 1 {
                return ai_error_with(
                    "PRECONDITION_FAILED",
                    "expected_artifacts>1 not supported; split jobs or use separate artifacts",
                    Some(
                        "Split outputs into separate jobs (one artifact per job), or ensure the dispatch macro sets a single expected_artifacts entry.",
                    ),
                    Vec::new(),
                );
            }
            let artifact_key = expected_artifacts
                .first()
                .cloned()
                .unwrap_or_else(String::new);
            let Some(s) = summary.as_deref() else {
                return ai_error_with(
                    "PRECONDITION_FAILED",
                    "jobs.complete: summary is required for DONE when expected_artifacts is set",
                    Some(
                        "Provide summary as a JSON object matching the expected artifact contract.",
                    ),
                    Vec::new(),
                );
            };
            let s = s.trim();
            if s.is_empty() {
                return ai_error_with(
                    "PRECONDITION_FAILED",
                    "jobs.complete: summary must be non-empty JSON when expected_artifacts is set",
                    Some(
                        "Provide summary as a JSON object matching the expected artifact contract.",
                    ),
                    Vec::new(),
                );
            }

            let raw_json = match crate::support::parse_json_object_from_text(
                s,
                "jobs.complete.summary (expected artifact JSON)",
            ) {
                Ok(v) => v,
                Err(_) => {
                    return ai_error_with(
                        "PRECONDITION_FAILED",
                        "jobs.complete: summary must be a JSON object when expected_artifacts is set",
                        Some(
                            "Return the artifact pack as a JSON object (root object), not free-form text.",
                        ),
                        Vec::new(),
                    );
                }
            };

            let (normalized, contract_warnings) = match crate::support::validate_by_artifact_key(
                &server.store,
                &workspace,
                &artifact_key,
                &raw_json,
                open.meta_json.as_deref(),
            ) {
                Ok(v) => v,
                Err(resp) => {
                    let details = resp
                        .get("error")
                        .and_then(|v| v.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("summary does not match the expected artifact contract");
                    return ai_error_with(
                        "PRECONDITION_FAILED",
                        details,
                        Some(&format!(
                            "jobs.complete: summary JSON does not match the expected artifact contract (artifact_key={artifact_key})"
                        )),
                        Vec::new(),
                    );
                }
            };
            completion_warnings.extend(contract_warnings);

            let canonical_text = serde_json::to_string_pretty(&normalized)
                .or_else(|_| serde_json::to_string(&normalized))
                .unwrap_or_else(|_| s.to_string());

            match server.store.job_artifact_create(
                &workspace,
                bm_storage::JobArtifactCreateRequest {
                    job_id: job_id.clone(),
                    artifact_key: artifact_key.clone(),
                    content_text: canonical_text.clone(),
                },
            ) {
                Ok(_) => {}
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
                Err(StoreError::InvalidInput(msg)) => {
                    return ai_error_with(
                        "PRECONDITION_FAILED",
                        "jobs.complete: expected artifact could not be stored",
                        Some(msg),
                        Vec::new(),
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let artifact_ref = format!("artifact://jobs/{}/{}", job_id, artifact_key);
            if refs.len() < 32 && !refs.iter().any(|r| r == &artifact_ref) {
                refs.push(artifact_ref);
            }
            canonical_summary = Some(canonical_text);
        }
    }
    // Keep job thread navigable even when callers omit refs (bounded, deterministic).
    if refs.len() < 32 && !refs.iter().any(|r| r == &job_id) {
        refs.push(job_id.clone());
    }

    // HIGH priority DONE guardrail: require at least one checkpoint + at least one proof ref.
    if server.jobs_high_done_proof_gate_enabled && status_norm == "DONE" {
        let job_row = match server
            .store
            .job_get(&workspace, bm_storage::JobGetRequest { id: job_id.clone() })
        {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        if job_row.priority.trim().eq_ignore_ascii_case("HIGH") {
            let has_checkpoint = match server.store.job_checkpoint_exists(
                &workspace,
                bm_storage::JobCheckpointExistsRequest { id: job_id.clone() },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let has_proof_ref = refs.iter().any(|r| {
                let t = r.trim_start();
                t.starts_with("LINK:") || t.starts_with("CMD:") || t.starts_with("FILE:")
            });

            if !has_checkpoint || !has_proof_ref {
                let mut missing = Vec::<&str>::new();
                if !has_checkpoint {
                    missing.push("checkpoint");
                }
                if !has_proof_ref {
                    missing.push("proof_ref");
                }
                let missing = missing.join("+");

                let mut suggestions = Vec::<Value>::new();
                suggestions.push(suggest_call(
                    "tasks_jobs_open",
                    "Open job to inspect events and current refs.",
                    "high",
                    json!({ "job": job_id.clone() }),
                ));
                suggestions.push(suggest_call(
                    "tasks_jobs_report",
                    "Emit a meaningful checkpoint (required for HIGH priority DONE).",
                    "high",
                    json!({
                        "job": job_id.clone(),
                        "runner_id": runner_id.clone(),
                        "claim_revision": claim_revision,
                        "kind": "checkpoint",
                        "message": "checkpoint",
                        "meta": { "step": { "command": "<command>", "result": "<result>" } }
                    }),
                ));

                return ai_error_with(
                    "PRECONDITION_FAILED",
                    &format!(
                        "HIGH priority DONE requires: 1 checkpoint + 1 proof ref (missing={missing})"
                    ),
                    Some(
                        "Add a checkpoint via jobs.report(kind=checkpoint, meta.step.*) and include at least one proof ref (LINK:/CMD:/FILE:) in refs, then retry jobs.complete.",
                    ),
                    suggestions,
                );
            }
        }
    }

    let meta_value = args_obj.get("meta").cloned().filter(|v| !v.is_null());
    let meta_json = meta_value
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());

    let done = match server.store.job_complete(
        &workspace,
        bm_storage::JobCompleteRequest {
            id: job_id,
            runner_id,
            claim_revision,
            status,
            summary: canonical_summary,
            refs,
            meta_json,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
        Err(StoreError::JobClaimMismatch { job_id, .. }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job claim mismatch (job_id={job_id})"),
                Some(
                    "The job lease was reclaimed or rotated. Re-claim the job to obtain a new claim_revision.",
                ),
                Vec::new(),
            );
        }
        Err(StoreError::JobAlreadyTerminal { job_id, status }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job already terminal (job_id={job_id}, status={status})"),
                Some("Open the job to inspect prior completion and the referenced artifacts."),
                Vec::new(),
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(done.job),
        "event": job_event_to_json(done.event)
    });
    push_warning_if(&mut completion_warnings, unknown_warning);
    if completion_warnings.is_empty() {
        ai_ok("tasks_jobs_complete", result)
    } else {
        ai_ok_with_warnings(
            "tasks_jobs_complete",
            result,
            completion_warnings,
            Vec::new(),
        )
    }
}
