#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(super) fn tool_tasks_jobs_artifact_put(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &["workspace", "job", "artifact_key", "content_text"],
        "jobs.artifact.put",
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
    let artifact_key = match require_string(args_obj, "artifact_key") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let content_text = match require_string(args_obj, "content_text") {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let artifact = match server.store.job_artifact_create(
        &workspace,
        bm_storage::JobArtifactCreateRequest {
            job_id: job_id.clone(),
            artifact_key: artifact_key.clone(),
            content_text: content_text.clone(),
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let mut warnings = Vec::<Value>::new();
    push_warning_if(&mut warnings, unknown_warning);
    let result = json!({
        "workspace": workspace.as_str(),
        "artifact": {
            "job_id": artifact.job_id,
            "artifact_key": artifact.artifact_key,
            "content_len": artifact.content_len,
            "created_at_ms": artifact.created_at_ms
        },
        "artifact_ref": format!("artifact://jobs/{}/{}", job_id, artifact_key)
    });

    if warnings.is_empty() {
        ai_ok("tasks_jobs_artifact_put", result)
    } else {
        ai_ok_with_warnings("tasks_jobs_artifact_put", result, warnings, Vec::new())
    }
}

pub(super) fn tool_tasks_jobs_artifact_get(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &["workspace", "job", "artifact_key", "offset", "max_chars"],
        "jobs.artifact.get",
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
    let artifact_key = match require_string(args_obj, "artifact_key") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let offset = match optional_i64(args_obj, "offset") {
        Ok(v) => v.unwrap_or(0).max(0) as usize,
        Err(resp) => return resp,
    };
    let max_chars = match optional_i64(args_obj, "max_chars") {
        Ok(v) => v.unwrap_or(4000).clamp(1, 4000) as usize,
        Err(resp) => return resp,
    };

    let resolved = match crate::support::resolve_job_artifact_text(
        &mut server.store,
        &workspace,
        &job_id,
        &artifact_key,
        offset,
        max_chars,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let mut warnings = Vec::<Value>::new();
    push_warning_if(&mut warnings, unknown_warning);
    warnings.extend(resolved.warnings);
    let result = json!({
        "workspace": workspace.as_str(),
        "artifact": {
            "job_id": resolved.job_id,
            "artifact_key": resolved.artifact_key,
            "content_len": resolved.content_len,
            "created_at_ms": resolved.created_at_ms,
            "source": match resolved.source {
                crate::support::JobArtifactSource::Store => "store",
                crate::support::JobArtifactSource::SummaryFallback => "summary_fallback"
            },
            "offset": resolved.offset
        },
        "content_text": resolved.content_text,
        "offset": resolved.offset,
        "truncated": resolved.truncated,
        "artifact_ref": format!("artifact://jobs/{}/{}", resolved.job_id, resolved.artifact_key)
    });

    if warnings.is_empty() {
        ai_ok("tasks_jobs_artifact_get", result)
    } else {
        ai_ok_with_warnings("tasks_jobs_artifact_get", result, warnings, Vec::new())
    }
}
