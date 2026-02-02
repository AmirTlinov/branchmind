#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn job_row_to_json(job: bm_storage::JobRow) -> Value {
    json!({
        "job_id": job.id,
        "revision": job.revision,
        "status": job.status,
        "title": job.title,
        "kind": job.kind,
        "priority": job.priority,
        "task": job.task_id,
        "anchor": job.anchor_id,
        "runner": job.runner,
        "claim_expires_at_ms": job.claim_expires_at_ms,
        "summary": job.summary,
        "created_at_ms": job.created_at_ms,
        "updated_at_ms": job.updated_at_ms,
        "completed_at_ms": job.completed_at_ms
    })
}

pub(super) fn job_event_to_json(event: bm_storage::JobEventRow) -> Value {
    let job_id = event.job_id;
    let seq = event.seq;
    let job_ref = format!("{job_id}@{seq}");

    let mut out = json!({
        "seq": seq,
        "job_id": job_id,
        "ref": job_ref,
        "ts_ms": event.ts_ms,
        "kind": event.kind,
        "message": event.message,
        "percent": event.percent,
        "refs": event.refs
    });

    if let Some(meta_json) = event.meta_json.as_deref()
        && let Ok(meta) = serde_json::from_str::<Value>(meta_json)
        && let Some(obj) = out.as_object_mut()
    {
        obj.insert("meta".to_string(), meta);
    }

    out
}
