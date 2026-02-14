#![forbid(unsafe_code)]

use super::*;
use serde_json::{Value, json};

pub(super) fn tool_tasks_jobs_claim(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "job",
            "runner_id",
            "runner",
            "allow_stale",
            "lease_ttl_ms",
        ],
        "jobs.claim",
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
    let runner_id = match optional_string(args_obj, "runner_id") {
        Ok(v) => v,
        Err(resp) => return resp,
    }
    .or_else(|| optional_string(args_obj, "runner").ok().flatten());
    let Some(runner_id) = runner_id
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
    else {
        return ai_error("INVALID_INPUT", "runner_id is required");
    };
    let lease_ttl_ms = match optional_i64(args_obj, "lease_ttl_ms") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let lease_ttl_ms = match lease_ttl_ms {
        Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "lease_ttl_ms must be > 0"),
        Some(v) => v as u64,
        None => 180_000,
    };
    let allow_stale = match optional_bool(args_obj, "allow_stale") {
        Ok(v) => v.unwrap_or(false),
        Err(resp) => return resp,
    };

    let claimed = match server.store.job_claim(
        &workspace,
        bm_storage::JobClaimRequest {
            id: job_id,
            runner_id,
            lease_ttl_ms,
            allow_stale,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
        Err(StoreError::JobNotClaimable { job_id, status }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job is not claimable (job_id={job_id}, status={status})"),
                Some("Open the job to see its current status; cancel/requeue if needed."),
                Vec::new(),
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(claimed.job),
        "event": job_event_to_json(claimed.event)
    });
    if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_claim", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_claim", result)
    }
}

pub(super) fn tool_tasks_jobs_message(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &["workspace", "job", "message", "refs", "fmt"],
        "jobs.message",
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
    let message = match require_string(args_obj, "message") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let mut refs = match optional_string_array(args_obj, "refs") {
        Ok(v) => v.unwrap_or_else(Vec::new),
        Err(resp) => return resp,
    };

    // Proof-first DX: if refs were forgotten but proof/refs are present in message text,
    // salvage stable references deterministically (reduces needless proof-gate loops).
    if !message.trim().is_empty() {
        refs = crate::salvage_job_completion_refs(&message, &job_id, &refs);
    }

    let mut warnings = Vec::<Value>::new();
    push_warning_if(&mut warnings, unknown_warning);

    // CODE_REF validation (fail-closed on invalid format; drift => warning + keep message).
    let mut normalized_refs = Vec::<String>::new();
    for r in refs {
        match parse_code_ref(&r) {
            Ok(None) => normalized_refs.push(r),
            Ok(Some(code_ref)) => match validate_code_ref(&server.store, &workspace, &code_ref) {
                Ok(v) => {
                    normalized_refs.push(v.normalized);
                    warnings.extend(v.warnings);
                }
                Err(resp) => return resp,
            },
            Err(resp) => return resp,
        }
    }

    let posted = match server.store.job_message(
        &workspace,
        bm_storage::JobMessageRequest {
            id: job_id,
            message,
            refs: normalized_refs,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
        Err(StoreError::JobNotMessageable { job_id, status }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job is not messageable (job_id={job_id}, status={status})"),
                Some(
                    "Open the job to inspect its status; message is allowed only for QUEUED/RUNNING jobs.",
                ),
                Vec::new(),
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(posted.job),
        "event": job_event_to_json(posted.event)
    });
    if warnings.is_empty() {
        ai_ok("tasks_jobs_message", result)
    } else {
        ai_ok_with_warnings("tasks_jobs_message", result, warnings, Vec::new())
    }
}

pub(super) fn tool_tasks_jobs_report(server: &mut McpServer, args: Value) -> Value {
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
            "lease_ttl_ms",
            "kind",
            "message",
            "percent",
            "refs",
            "meta",
        ],
        "jobs.report",
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
    let lease_ttl_ms = match optional_i64(args_obj, "lease_ttl_ms") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let lease_ttl_ms = match lease_ttl_ms {
        Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "lease_ttl_ms must be > 0"),
        Some(v) => v as u64,
        None => 180_000,
    };
    let kind = match optional_string(args_obj, "kind") {
        Ok(v) => v.unwrap_or_else(|| "progress".to_string()),
        Err(resp) => return resp,
    };
    let kind_norm = kind.trim().to_ascii_lowercase();
    let message = match require_string(args_obj, "message") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let percent = match optional_i64(args_obj, "percent") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let refs = match optional_string_array(args_obj, "refs") {
        Ok(v) => v.unwrap_or_else(Vec::new),
        Err(resp) => return resp,
    };
    let meta_value = args_obj.get("meta").cloned().filter(|v| !v.is_null());

    if server.jobs_strict_progress_schema_enabled
        && matches!(kind_norm.as_str(), "progress" | "checkpoint")
    {
        let Some(meta) = meta_value.as_ref() else {
            return ai_error(
                "INVALID_INPUT",
                "meta is required for kind=progress|checkpoint (meta.step.command + meta.step.result|meta.step.error)",
            );
        };
        let Some(meta_obj) = meta.as_object() else {
            return ai_error(
                "INVALID_INPUT",
                "meta must be an object for kind=progress|checkpoint",
            );
        };
        let Some(step) = meta_obj.get("step").and_then(|v| v.as_object()) else {
            return ai_error(
                "INVALID_INPUT",
                "meta.step is required (object) for kind=progress|checkpoint",
            );
        };
        let command_ok = step
            .get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        let has_result = step.get("result").is_some_and(|v| !v.is_null());
        let has_error = step.get("error").is_some_and(|v| !v.is_null());
        if !command_ok || (!has_result && !has_error) {
            return ai_error(
                "INVALID_INPUT",
                "meta.step.command and (meta.step.result or meta.step.error) are required for kind=progress|checkpoint",
            );
        }
    }

    let meta_json = meta_value
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());

    let report = match server.store.job_report(
        &workspace,
        bm_storage::JobReportRequest {
            id: job_id,
            runner_id,
            claim_revision,
            kind,
            message,
            percent,
            refs,
            meta_json,
            lease_ttl_ms,
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
        Err(StoreError::JobNotRunning { job_id, status }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job is not running (job_id={job_id}, status={status})"),
                Some("Open the job to see its current status; claim it or complete/cancel it."),
                Vec::new(),
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(report.job),
        "event": job_event_to_json(report.event)
    });
    if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_report", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_report", result)
    }
}
