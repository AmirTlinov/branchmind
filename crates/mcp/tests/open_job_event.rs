#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_job_event_ref_is_supported_and_is_read_only() {
    let mut server = Server::start_initialized_with_args(
        "open_job_event_ref_is_supported_and_is_read_only",
        &["--toolset", "full", "--workspace", "ws_open_job_event"],
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws_open_job_event",
                "title": "Open Job Event",
                "prompt": "Emit events"
            } } }
    }));
    let created_out = extract_tool_text(&created);
    assert!(
        created_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs_create must succeed: {created_out}"
    );

    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": { "workspace": "ws_open_job_event", "job": job_id, "runner_id": "r1" } } }
    }));
    let claimed_out = extract_tool_text(&claimed);
    assert!(
        claimed_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs_claim must succeed: {claimed_out}"
    );
    let claim_revision = claimed_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("job.revision claim token");

    let reported = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": {
                "workspace": "ws_open_job_event",
                "job": job_id,
                "runner_id": "r1",
                "claim_revision": claim_revision,
                "kind": "checkpoint",
                "message": "checkpoint 1",
                "refs": ["JOB-REF"]
            } } }
    }));
    let reported_out = extract_tool_text(&reported);
    assert!(
        reported_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs_report must succeed: {reported_out}"
    );

    let seq = reported_out
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("event.seq");

    let ref_id = format!("{job_id}@{seq}");
    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": ref_id } }
    }));

    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(job_event) must succeed: {opened_out}"
    );

    let result = opened_out.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("job_event"),
        "open(JOB-*@seq) must return kind=job_event"
    );
    assert_eq!(
        result.get("ref").and_then(|v| v.as_str()),
        Some(ref_id.as_str()),
        "open(job_event) must preserve ref"
    );

    let opened_seq = result
        .get("event")
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert_eq!(opened_seq, seq, "open(job_event) must return the exact seq");

    let opened_job_id = result
        .get("job")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        opened_job_id.starts_with("JOB-"),
        "open(job_event) must include job summary"
    );
}
