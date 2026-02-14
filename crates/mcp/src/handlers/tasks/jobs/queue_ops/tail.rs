#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

pub(crate) fn tool_tasks_jobs_tail(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &["workspace", "job", "after_seq", "limit", "max_chars", "fmt"],
        "jobs.tail",
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
    let after_seq = match optional_i64(args_obj, "after_seq") {
        Ok(v) => v.unwrap_or(0),
        Err(resp) => return resp,
    };
    if after_seq < 0 {
        return ai_error("INVALID_INPUT", "after_seq must be >= 0");
    }
    let limit = match optional_usize(args_obj, "limit") {
        Ok(v) => v.unwrap_or(50).clamp(1, 200),
        Err(resp) => return resp,
    };
    let max_chars = match optional_usize(args_obj, "max_chars") {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let tail = match server.store.job_events_tail(
        &workspace,
        bm_storage::JobEventsTailRequest {
            id: job_id.clone(),
            after_seq,
            limit,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let events_json = tail
        .events
        .into_iter()
        .map(job_event_to_json)
        .collect::<Vec<_>>();
    let mut result = json!({
        "workspace": workspace.as_str(),
        "job_id": tail.job_id,
        "after_seq": tail.after_seq,
        "next_after_seq": tail.next_after_seq,
        "events": events_json,
        "count": events_json.len(),
        "has_more": tail.has_more,
        "truncated": false
    });

    if let Some(limit) = max_chars {
        let (limit, clamped) = clamp_budget_max(limit);
        let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "events", limit);
        if let Some(obj) = result.as_object_mut()
            && let Some(events) = obj.get("events").and_then(|v| v.as_array())
        {
            obj.insert(
                "count".to_string(),
                Value::Number(serde_json::Number::from(events.len() as u64)),
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
            ai_ok("tasks_jobs_tail", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_tail", result, warnings, Vec::new())
        }
    } else if let Some(w) = unknown_warning {
        ai_ok_with_warnings("tasks_jobs_tail", result, vec![w], Vec::new())
    } else {
        ai_ok("tasks_jobs_tail", result)
    }
}
