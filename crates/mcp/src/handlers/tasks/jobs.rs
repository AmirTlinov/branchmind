#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

mod artifact_ops;
mod queue_ops;
mod reporting_ops;
mod runner_ops;

mod executor_routing;
mod terminal_ops;

mod control_center;
mod macros;
mod mesh;
mod pipeline;
mod pipeline_ab;
mod pipeline_apply;
mod pipeline_v2;
mod radar;

fn job_row_to_json(job: bm_storage::JobRow) -> Value {
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

fn job_event_to_json(event: bm_storage::JobEventRow) -> Value {
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

fn runner_lease_to_json(row: bm_storage::RunnerLeaseRow) -> Value {
    json!({
        "runner_id": row.runner_id,
        "status": row.status,
        "active_job_id": row.active_job_id,
        "lease_expires_at_ms": row.lease_expires_at_ms,
        "created_at_ms": row.created_at_ms,
        "updated_at_ms": row.updated_at_ms
    })
}

fn runner_lease_offline_to_json(row: bm_storage::RunnerLeaseRow) -> Value {
    json!({
        "runner_id": row.runner_id,
        "status": "offline",
        "last_status": row.status,
        "active_job_id": row.active_job_id,
        "lease_expires_at_ms": row.lease_expires_at_ms,
        "created_at_ms": row.created_at_ms,
        "updated_at_ms": row.updated_at_ms
    })
}

fn runner_status_to_json(s: bm_storage::RunnerStatusSnapshot) -> Value {
    json!({
        "status": s.status,
        "live_count": s.live_count,
        "idle_count": s.idle_count,
        "offline_count": s.offline_count,
        "runner_id": s.runner_id,
        "active_job_id": s.active_job_id,
        "lease_expires_at_ms": s.lease_expires_at_ms
    })
}

fn check_unknown_args(
    args_obj: &serde_json::Map<String, Value>,
    allowed: &[&str],
    cmd: &str,
    fail_closed: bool,
) -> Result<Option<Value>, Value> {
    // Envelope/budget machinery may inject these keys for bounded responses; they are not
    // user-facing semantic args and should not trigger unknown-args failures.
    const IMPLICIT_ENVELOPE_KEYS: &[&str] = &["context_budget", "limit", "max_chars", "agent_id"];
    let mut unknown = args_obj
        .keys()
        .filter(|k| {
            !allowed.iter().any(|a| a == &k.as_str())
                && !IMPLICIT_ENVELOPE_KEYS.iter().any(|ik| ik == &k.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    unknown.dedup();
    if unknown.is_empty() {
        return Ok(None);
    }

    if fail_closed {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("unknown args: {}", unknown.join(", ")),
            Some(&format!(
                "Remove unknown args or inspect schema via system(op=schema.get args={{cmd:\"{cmd}\"}})."
            )),
            Vec::new(),
        ));
    }

    Ok(Some(warning(
        "UNKNOWN_ARGS_IGNORED",
        &format!("unknown args ignored: {}", unknown.join(", ")),
        &format!(
            "Remove unknown args or inspect schema via system(op=schema.get args={{cmd:\"{cmd}\"}})."
        ),
    )))
}

fn push_warning_if(warnings: &mut Vec<Value>, warning_value: Option<Value>) {
    if let Some(w) = warning_value {
        warnings.push(w);
    }
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_create(&mut self, args: Value) -> Value {
        queue_ops::tool_tasks_jobs_create(self, args)
    }

    pub(crate) fn tool_tasks_jobs_list(&mut self, args: Value) -> Value {
        queue_ops::tool_tasks_jobs_list(self, args)
    }

    pub(crate) fn tool_tasks_jobs_artifact_put(&mut self, args: Value) -> Value {
        artifact_ops::tool_tasks_jobs_artifact_put(self, args)
    }

    pub(crate) fn tool_tasks_jobs_artifact_get(&mut self, args: Value) -> Value {
        artifact_ops::tool_tasks_jobs_artifact_get(self, args)
    }

    pub(crate) fn tool_tasks_runner_heartbeat(&mut self, args: Value) -> Value {
        runner_ops::tool_tasks_runner_heartbeat(self, args)
    }

    pub(crate) fn tool_tasks_jobs_open(&mut self, args: Value) -> Value {
        queue_ops::tool_tasks_jobs_open(self, args)
    }

    pub(crate) fn tool_tasks_jobs_tail(&mut self, args: Value) -> Value {
        queue_ops::tool_tasks_jobs_tail(self, args)
    }

    pub(crate) fn tool_tasks_jobs_complete(&mut self, args: Value) -> Value {
        terminal_ops::tool_tasks_jobs_complete(self, args)
    }

    pub(crate) fn tool_tasks_jobs_requeue(&mut self, args: Value) -> Value {
        terminal_ops::tool_tasks_jobs_requeue(self, args)
    }

    pub(crate) fn tool_tasks_jobs_macro_rotate_stalled(&mut self, args: Value) -> Value {
        terminal_ops::tool_tasks_jobs_macro_rotate_stalled(self, args)
    }

    pub(crate) fn tool_tasks_jobs_claim(&mut self, args: Value) -> Value {
        reporting_ops::tool_tasks_jobs_claim(self, args)
    }

    pub(crate) fn tool_tasks_jobs_message(&mut self, args: Value) -> Value {
        reporting_ops::tool_tasks_jobs_message(self, args)
    }

    pub(crate) fn tool_tasks_jobs_report(&mut self, args: Value) -> Value {
        reporting_ops::tool_tasks_jobs_report(self, args)
    }
}
