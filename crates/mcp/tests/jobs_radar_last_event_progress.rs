#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

fn claim_revision(server: &mut Server, workspace: &str, job_id: &str, runner_id: &str) -> i64 {
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": { "workspace": workspace, "job": job_id, "runner_id": runner_id } } }
    }));
    let out = extract_tool_text(&resp);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_claim must succeed: {out}"
    );
    out.get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("job.revision claim token")
}

#[test]
fn jobs_radar_last_event_prefers_progress_over_stale_error() {
    let mut server =
        Server::start_initialized("jobs_radar_last_event_prefers_progress_over_stale_error");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Radar last-event smoke", "prompt": "noop" } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_revision(&mut server, "ws1", &job_id, "r1");

    // Write an error first (historical), then a checkpoint, then progress.
    // jobs_radar must *not* keep showing the stale error as `last` once it is checkpointed.
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "error", "message": "boom" } } }
    }));
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "checkpoint", "message": "checkpoint ok" } } }
    }));
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "progress", "message": "still working", "percent": 10 } } }
    }));

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10 } } }
    }));
    let radar_out = extract_tool_text(&radar);
    let jobs = radar_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let row = jobs
        .iter()
        .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job_id.as_str()))
        .expect("radar must include created job");

    let last_kind = row
        .get("last")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(
        last_kind, "progress",
        "expected last.kind=progress after checkpoint cleared the earlier error"
    );

    let has_error = row
        .get("attention")
        .and_then(|v| v.get("has_error"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        !has_error,
        "expected attention.has_error=false after checkpoint: {row}"
    );
}
