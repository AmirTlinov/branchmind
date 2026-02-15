#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_job(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    job_id: &str,
    include_prompt: bool,
    limit: usize,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    let max_events = limit.clamp(1, 50);
    let opened = match server.store.job_open(
        workspace,
        bm_storage::JobOpenRequest {
            id: job_id.to_string(),
            include_prompt,
            include_events: true,
            include_meta: true,
            max_events,
            before_seq: None,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(StoreError::UnknownId) => {
            return Err(ai_error_with(
                "UNKNOWN_ID",
                "Unknown job id",
                Some("Copy a JOB-* id from tasks_snapshot or tasks_jobs_list."),
                vec![],
            ));
        }
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let events = opened
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

    if !include_prompt {
        suggestions.push(json!({
            "tool": "tasks_jobs_open",
            "reason": "Open full job spec (prompt) and more events",
            "args_hint": {
                "workspace": workspace.as_str(),
                "job": job_id,
                "include_prompt": true,
                "include_events": true,
                "max_events": max_events
            }
        }));
    }

    if opened.has_more_events
        && let Some(oldest) = opened.events.last()
    {
        suggestions.push(json!({
            "tool": "tasks_jobs_open",
            "reason": "Page older job events",
            "args_hint": {
                "workspace": workspace.as_str(),
                "job": job_id,
                "include_prompt": false,
                "include_events": true,
                "max_events": max_events,
                "before_seq": oldest.seq
            }
        }));
    }

    Ok(json!({
        "workspace": workspace.as_str(),
        "kind": "job",
        "id": job_id,
        "job": {
            "id": opened.job.id,
            "revision": opened.job.revision,
            "status": opened.job.status,
            "title": opened.job.title,
            "kind": opened.job.kind,
            "priority": opened.job.priority,
            "task_id": opened.job.task_id,
            "anchor_id": opened.job.anchor_id,
            "runner": opened.job.runner,
            "summary": opened.job.summary,
            "created_at_ms": opened.job.created_at_ms,
            "updated_at_ms": opened.job.updated_at_ms,
            "completed_at_ms": opened.job.completed_at_ms
        },
        "prompt": opened.prompt,
        "meta": opened.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
        "events": events,
        "has_more_events": opened.has_more_events,
        "truncated": false
    }))
}
