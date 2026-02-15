#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::OpenJobEventRefArgs;

pub(super) fn open_job_event_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args: OpenJobEventRefArgs<'_>,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    let job_row = match server.store.job_get(
        workspace,
        bm_storage::JobGetRequest {
            id: args.job_id.to_string(),
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(job) = job_row else {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown job id",
            Some("Copy a JOB-* id from tasks_snapshot or tasks_jobs_list."),
            vec![],
        ));
    };

    let event_row = match server.store.job_event_get(
        workspace,
        bm_storage::JobEventGetRequest {
            job_id: args.job_id.to_string(),
            seq: args.seq,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let Some(event) = event_row else {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            "Unknown job event ref",
            Some("Open the job and copy a valid event seq."),
            vec![],
        ));
    };

    let job_ref = format!("{}@{}", args.job_id, args.seq);
    let mut event_json = json!({
        "ref": job_ref,
        "seq": event.seq,
        "ts": ts_ms_to_rfc3339(event.ts_ms),
        "ts_ms": event.ts_ms,
        "kind": event.kind,
        "message": event.message,
        "percent": event.percent,
        "refs": event.refs
    });
    if let Some(meta_json) = event.meta_json.as_deref()
        && let Ok(meta) = serde_json::from_str::<Value>(meta_json)
        && let Some(obj) = event_json.as_object_mut()
    {
        obj.insert("meta".to_string(), meta);
    }

    let max_events = args.limit.clamp(1, 50);
    let ctx = match server.store.job_open(
        workspace,
        bm_storage::JobOpenRequest {
            id: args.job_id.to_string(),
            include_prompt: false,
            include_events: true,
            include_meta: false,
            max_events,
            before_seq: Some(args.seq.saturating_add(1)),
        },
    ) {
        Ok(v) => v,
        Err(_) => bm_storage::JobOpenResult {
            job: job.clone(),
            prompt: None,
            meta_json: None,
            events: Vec::new(),
            has_more_events: false,
        },
    };

    let ctx_events = ctx
        .events
        .iter()
        .map(|e| {
            let job_ref = format!("{}@{}", e.job_id, e.seq);
            json!({
                "ref": job_ref,
                "seq": e.seq,
                "ts": ts_ms_to_rfc3339(e.ts_ms),
                "ts_ms": e.ts_ms,
                "kind": e.kind,
                "message": e.message,
                "percent": e.percent,
                "refs": e.refs
            })
        })
        .collect::<Vec<_>>();

    let ctx_count = ctx_events.len();

    suggestions.push(json!({
        "tool": "tasks_jobs_tail",
        "reason": "Follow job events incrementally (no lose-place loops)",
        "args_hint": {
            "workspace": workspace.as_str(),
            "job": args.job_id,
            "after_seq": args.seq,
            "limit": 50,
            "max_chars": 4000
        }
    }));

    suggestions.push(json!({
        "tool": "tasks_jobs_open",
        "reason": "Open the job (status + prompt + recent events)",
        "args_hint": {
            "workspace": workspace.as_str(),
            "job": args.job_id,
            "include_prompt": args.include_drafts,
            "include_events": true,
            "max_events": max_events,
            "max_chars": 8000
        }
    }));

    Ok(json!({
        "workspace": workspace.as_str(),
        "kind": "job_event",
        "ref": args.ref_str,
        "job": {
            "id": job.id,
            "revision": job.revision,
            "status": job.status,
            "title": job.title,
            "kind": job.kind,
            "priority": job.priority,
            "task_id": job.task_id,
            "anchor_id": job.anchor_id,
            "runner": job.runner,
            "summary": job.summary,
            "created_at_ms": job.created_at_ms,
            "updated_at_ms": job.updated_at_ms,
            "completed_at_ms": job.completed_at_ms
        },
        "event": event_json,
        "context": {
            "events": ctx_events,
            "count": ctx_count,
            "has_more_events": ctx.has_more_events
        },
        "truncated": false
    }))
}
