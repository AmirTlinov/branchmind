#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_requeue(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &["workspace", "job", "reason", "refs", "meta"],
        "jobs.requeue",
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
    let reason = match optional_string(args_obj, "reason") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let refs = match optional_string_array(args_obj, "refs") {
        Ok(v) => v.unwrap_or_else(Vec::new),
        Err(resp) => return resp,
    };
    let meta_json = args_obj
        .get("meta")
        .cloned()
        .and_then(|v| serde_json::to_string(&v).ok());

    let requeued = match server.store.job_requeue(
        &workspace,
        bm_storage::JobRequeueRequest {
            id: job_id,
            reason,
            refs,
            meta_json,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
        Err(StoreError::JobNotRequeueable { job_id, status }) => {
            return ai_error_with(
                "CONFLICT",
                &format!("job is not requeueable (job_id={job_id}, status={status})"),
                Some("Open the job to see its current status; cancel/complete it first if needed."),
                Vec::new(),
            );
        }
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let result = json!({
        "workspace": workspace.as_str(),
        "job": job_row_to_json(requeued.job),
        "event": job_event_to_json(requeued.event)
    });
    if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_requeue", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_requeue", result)
    }
}
