#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn create_job(server: &mut Server, workspace: &str) -> String {
    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "create",
                "args": {
                    "title": "test job",
                    "prompt": "do nothing"
                }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    text.get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string()
}

#[test]
fn jobs_cancel_queued_transitions_to_canceled() {
    let mut server = Server::start_initialized("jobs_cancel_queued_transitions_to_canceled");
    let ws = "ws_jobs_cancel_queued";
    let job_id = create_job(&mut server, ws);

    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "cancel",
                "args": {
                    "job": job_id,
                    "reason": "stop",
                    "refs": ["REF:cancel-test"]
                }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("job"))
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str()),
        Some("CANCELED")
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("event"))
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("canceled")
    );
}

#[test]
fn jobs_cancel_running_returns_actionable_recovery_actions() {
    let mut server =
        Server::start_initialized("jobs_cancel_running_returns_actionable_recovery_actions");
    let ws = "ws_jobs_cancel_running";
    let job_id = create_job(&mut server, ws);

    // Claim (QUEUED -> RUNNING) so cancel must fail (queued-only).
    let claim = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "call",
                "cmd": "jobs.claim",
                "args": { "job": job_id, "runner_id": "runner_1" }
            }
        }
    }));
    let claim_text = extract_tool_text(&claim);
    let claim_revision = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("result.job.revision");

    let cancel = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "cancel",
                "args": { "job": job_id, "reason": "stop" }
            }
        }
    }));
    let cancel_text = extract_tool_text(&cancel);
    assert_eq!(
        cancel_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("CONFLICT")
    );

    let actions = cancel_text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions must be present");

    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("jobs")
                && a.get("args")
                    .and_then(|v| v.get("op"))
                    .and_then(|v| v.as_str())
                    == Some("open")
        }),
        "expected jobs.open recovery action"
    );

    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("jobs")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("jobs.complete")
                && a.get("args")
                    .and_then(|v| v.get("args"))
                    .and_then(|v| v.get("runner_id"))
                    .and_then(|v| v.as_str())
                    == Some("runner_1")
                && a.get("args")
                    .and_then(|v| v.get("args"))
                    .and_then(|v| v.get("claim_revision"))
                    .and_then(|v| v.as_i64())
                    == Some(claim_revision)
        }),
        "expected prefilled jobs.complete recovery action"
    );
}

#[test]
fn jobs_wait_timeout_zero_returns_done_false_for_nonterminal() {
    let mut server =
        Server::start_initialized("jobs_wait_timeout_zero_returns_done_false_for_nonterminal");
    let ws = "ws_jobs_wait_queued";
    let job_id = create_job(&mut server, ws);

    let resp = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "wait",
                "args": { "job": job_id, "timeout_ms": 0 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("done"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("job"))
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str()),
        Some("QUEUED")
    );
}

#[test]
fn jobs_wait_timeout_zero_returns_done_true_for_terminal() {
    let mut server =
        Server::start_initialized("jobs_wait_timeout_zero_returns_done_true_for_terminal");
    let ws = "ws_jobs_wait_done";
    let job_id = create_job(&mut server, ws);

    // Claim + complete (DONE).
    let claim = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "call",
                "cmd": "jobs.claim",
                "args": { "job": job_id, "runner_id": "runner_1" }
            }
        }
    }));
    let claim_text = extract_tool_text(&claim);
    let claim_revision = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("result.job.revision");

    let _complete = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "call",
                "cmd": "jobs.complete",
                "args": { "job": job_id, "runner_id": "runner_1", "claim_revision": claim_revision, "status": "DONE" }
            }
        }
    }));

    let wait = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": ws,
                "op": "wait",
                "args": { "job": job_id, "timeout_ms": 0 }
            }
        }
    }));
    let wait_text = extract_tool_text(&wait);
    assert_eq!(
        wait_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        wait_text
            .get("result")
            .and_then(|v| v.get("done"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        wait_text
            .get("result")
            .and_then(|v| v.get("job"))
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str()),
        Some("DONE")
    );
}
