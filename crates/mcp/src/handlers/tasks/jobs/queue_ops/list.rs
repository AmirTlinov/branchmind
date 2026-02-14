#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_list(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let workspace = match require_workspace(args_obj) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "status",
            "task",
            "anchor",
            "limit",
            "max_chars",
            "fmt",
        ],
        "jobs.list",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let status = match optional_string(args_obj, "status") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let task_id = match optional_string(args_obj, "task") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let anchor_id = match optional_string(args_obj, "anchor") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let limit = match optional_usize(args_obj, "limit") {
        Ok(v) => v.unwrap_or(25).clamp(1, 200),
        Err(resp) => return resp,
    };
    let max_chars = match optional_usize(args_obj, "max_chars") {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let list = match server.store.jobs_list(
        &workspace,
        bm_storage::JobsListRequest {
            status,
            task_id,
            anchor_id,
            limit,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let jobs_json = list
        .jobs
        .into_iter()
        .map(job_row_to_json)
        .collect::<Vec<_>>();
    let mut result = json!({
        "workspace": workspace.as_str(),
        "jobs": jobs_json,
        "count": jobs_json.len(),
        "has_more": list.has_more,
        "truncated": false
    });

    if let Some(limit) = max_chars {
        let (limit, clamped) = clamp_budget_max(limit);
        let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "jobs", limit);

        if let Some(obj) = result.as_object_mut()
            && let Some(jobs) = obj.get("jobs").and_then(|v| v.as_array())
        {
            obj.insert(
                "count".to_string(),
                Value::Number(serde_json::Number::from(jobs.len() as u64)),
            );
            let has_more = obj
                .get("has_more")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if budget_truncated && !has_more {
                obj.insert("has_more".to_string(), Value::Bool(true));
            }
        }

        set_truncated_flag(&mut result, budget_truncated);
        let _used = attach_budget(&mut result, limit, budget_truncated);

        let mut warnings = budget_warnings(budget_truncated, false, clamped);
        push_warning_if(&mut warnings, unknown_warning);
        if warnings.is_empty() {
            ai_ok("tasks_jobs_list", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_list", result, warnings, Vec::new())
        }
    } else if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_list", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_list", result)
    }
}
