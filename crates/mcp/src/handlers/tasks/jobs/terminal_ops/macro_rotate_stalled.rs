#![forbid(unsafe_code)]

use crate::handlers::tasks::jobs::*;
use serde_json::{Value, json};

fn rotated_job_meta_json(old_meta_json: Option<String>, from_job_id: &str) -> Option<String> {
    let mut out = old_meta_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if let Some(raw) = old_meta_json.as_deref()
        && let Ok(v) = serde_json::from_str::<Value>(raw)
        && let Some(obj) = v.as_object()
    {
        for (key, value) in obj {
            out.insert(key.clone(), value.clone());
        }
    }
    out.insert(
        "rotated_from".to_string(),
        Value::String(from_job_id.to_string()),
    );
    out.insert(
        "rotation_reason".to_string(),
        Value::String("rotate_stalled".to_string()),
    );
    serde_json::to_string(&Value::Object(out)).ok()
}

pub(crate) fn tool_tasks_jobs_macro_rotate_stalled(server: &mut McpServer, args: Value) -> Value {
    let Some(args_obj) = args.as_object() else {
        return ai_error("INVALID_INPUT", "arguments must be an object");
    };
    let unknown_warning = match check_unknown_args(
        args_obj,
        &[
            "workspace",
            "stall_after_s",
            "stale_after_s",
            "limit",
            "dry_run",
        ],
        "jobs.macro.rotate.stalled",
        server.jobs_unknown_args_fail_closed_enabled,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let workspace = match require_workspace(args_obj) {
        Ok(w) => w,
        Err(resp) => return resp,
    };

    let stall_after_s = match optional_usize(args_obj, "stall_after_s") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let stale_after_s = match optional_usize(args_obj, "stale_after_s") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if stall_after_s.is_some() && stale_after_s.is_some() {
        return ai_error(
            "INVALID_INPUT",
            "use only one of stall_after_s or stale_after_s",
        );
    }
    let mut warnings = Vec::<Value>::new();
    if stale_after_s.is_some() {
        warnings.push(warning(
            "DEPRECATED_ARG",
            "stale_after_s is deprecated; use stall_after_s",
            "Rename stale_after_s -> stall_after_s.",
        ));
    }
    if let Some(w) = unknown_warning {
        warnings.push(w);
    }

    let stall_after_input = stall_after_s.or(stale_after_s).unwrap_or(600);
    let stall_after_s = stall_after_input.clamp(60, 86_400) as i64;
    if stall_after_input != stall_after_s as usize {
        warnings.push(warning(
            "ARG_COERCED",
            &format!("stall_after_s coerced to {}", stall_after_s),
            "Use a value in range [60..86400].",
        ));
    }
    let stall_after_ms = stall_after_s.saturating_mul(1000);
    let now_ms = crate::support::now_ms_i64();

    let limit = match optional_usize(args_obj, "limit") {
        Ok(v) => v.unwrap_or(5).clamp(1, 50),
        Err(resp) => return resp,
    };
    let dry_run = match optional_bool(args_obj, "dry_run") {
        Ok(v) => v.unwrap_or(false),
        Err(resp) => return resp,
    };

    // Pull a bounded RUNNING set and rotate the stalled head (deterministic ordering).
    let scan_limit = limit.saturating_mul(8).clamp(limit, 200);
    let radar = match server.store.jobs_radar(
        &workspace,
        bm_storage::JobsRadarRequest {
            status: Some("RUNNING".to_string()),
            task_id: None,
            anchor_id: None,
            limit: scan_limit,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
    };

    let mut rotated = Vec::<Value>::new();
    let mut skipped = Vec::<Value>::new();

    for row in radar.rows {
        if rotated.len() >= limit {
            break;
        }
        let bm_storage::JobRadarRow {
            job,
            last_event,
            last_checkpoint_ts_ms,
            ..
        } = row;

        if job.status != "RUNNING" {
            continue;
        }

        let stale = job.claim_expires_at_ms.map(|v| v <= now_ms).unwrap_or(true);
        let meaningful_at_ms = last_checkpoint_ts_ms
            .or_else(|| last_event.as_ref().map(|e| e.ts_ms))
            .unwrap_or(job.updated_at_ms);
        let meaningful_age_ms = now_ms.saturating_sub(meaningful_at_ms);
        let stalled = !stale && meaningful_age_ms >= stall_after_ms;

        if !stalled {
            skipped.push(json!({
                "job_id": job.id,
                "reason": "not_stalled",
                "meaningful_at_ms": meaningful_at_ms,
                "meaningful_age_ms": meaningful_age_ms,
                "checkpoint_at_ms": last_checkpoint_ts_ms
            }));
            continue;
        }

        let from_job_id = job.id.clone();
        if dry_run {
            rotated.push(json!({
                "from_job_id": from_job_id,
                "dry_run": true,
                "meaningful_at_ms": meaningful_at_ms,
                "meaningful_age_ms": meaningful_age_ms,
                "checkpoint_at_ms": last_checkpoint_ts_ms
            }));
            continue;
        }

        let opened = match server.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: from_job_id.clone(),
                include_prompt: true,
                include_events: false,
                include_meta: true,
                max_events: 0,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => {
                skipped.push(json!({ "job_id": from_job_id, "reason": "unknown_id" }));
                continue;
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let prompt = opened.prompt.unwrap_or_default();
        if prompt.trim().is_empty() {
            skipped.push(json!({
                "job_id": from_job_id,
                "reason": "missing_prompt"
            }));
            continue;
        }

        let cancel_meta_json = serde_json::to_string(&json!({
            "macro": "rotate_stalled",
            "stall_after_s": stall_after_s,
            "meaningful_at_ms": meaningful_at_ms,
            "checkpoint_at_ms": last_checkpoint_ts_ms
        }))
        .ok();

        let canceled = match server.store.job_cancel(
            &workspace,
            bm_storage::JobCancelRequest {
                id: from_job_id.clone(),
                reason: Some(format!(
                    "rotate_stalled: no meaningful checkpoint/progress for >{}s",
                    stall_after_s
                )),
                refs: vec![from_job_id.clone()],
                meta_json: cancel_meta_json,
                force_running: true,
                expected_revision: Some(job.revision),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::RevisionMismatch { expected, actual }) => {
                skipped.push(json!({
                    "job_id": from_job_id,
                    "reason": "revision_mismatch",
                    "expected": expected,
                    "actual": actual
                }));
                continue;
            }
            Err(StoreError::JobAlreadyTerminal { job_id, status }) => {
                skipped.push(json!({
                    "job_id": job_id,
                    "reason": "already_terminal",
                    "status": status
                }));
                continue;
            }
            Err(StoreError::JobNotCancelable { job_id, status }) => {
                skipped.push(json!({
                    "job_id": job_id,
                    "reason": "not_cancelable",
                    "status": status
                }));
                continue;
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let new_meta_json = rotated_job_meta_json(opened.meta_json, &from_job_id);

        let created = match server.store.job_create(
            &workspace,
            bm_storage::JobCreateRequest {
                title: job.title,
                prompt,
                kind: job.kind,
                priority: job.priority,
                task_id: job.task_id,
                anchor_id: job.anchor_id,
                meta_json: new_meta_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        rotated.push(json!({
            "from_job_id": from_job_id,
            "canceled": {
                "job": job_row_to_json(canceled.job),
                "event": job_event_to_json(canceled.event)
            },
            "created": {
                "job": job_row_to_json(created.job),
                "event": job_event_to_json(created.created_event)
            }
        }));
    }

    let result = json!({
        "workspace": workspace.as_str(),
        "dry_run": dry_run,
        "stall_after_s": stall_after_s,
        "counts": {
            "rotated": rotated.len(),
            "skipped": skipped.len()
        },
        "rotated": rotated,
        "skipped": skipped
    });

    let mut suggestions = Vec::<Value>::new();
    suggestions.push(suggest_call(
        "tasks_jobs_radar",
        "Refresh radar after rotate.",
        "medium",
        json!({}),
    ));

    if warnings.is_empty() {
        ai_ok_with("tasks_jobs_macro_rotate_stalled", result, suggestions)
    } else {
        ai_ok_with_warnings(
            "tasks_jobs_macro_rotate_stalled",
            result,
            warnings,
            suggestions,
        )
    }
}
