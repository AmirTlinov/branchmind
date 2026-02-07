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
fn jobs_wait_mode_watch_returns_lines_and_copy_paste_continuation() {
    let mut server =
        Server::start_initialized("jobs_wait_mode_watch_returns_lines_and_copy_paste_continuation");
    let ws = "ws_jobs_wait_watch";
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
                "args": { "job": job_id, "timeout_ms": 0, "mode": "watch" }
            }
        }
    }));

    let rendered = extract_tool_text_str(&resp);
    assert!(
        !rendered.trim_start().starts_with('{'),
        "expected line-protocol text (not JSON), got:\n{rendered}"
    );

    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "non-terminal job should render 2 lines (status + continuation), got:\n{rendered}"
    );
    assert!(
        lines[0].contains("QUEUED")
            && lines[0].contains("done=false")
            && lines[0].contains("stop:"),
        "expected a status line with stop-condition hints, got:\n{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("jobs ")
            && lines[1].contains("op=wait")
            && lines[1].contains("\"mode\":\"watch\""),
        "expected a copy/paste continuation command, got:\n{}",
        lines[1]
    );

    // Copy/paste validation: parse the continuation line and ensure it dispatches without an
    // INVALID_INPUT/UNKNOWN_OP error.
    let (tool, args) = parse_portal_command_line(lines[1]);
    assert_eq!(tool, "jobs");
    let follow = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": tool, "arguments": serde_json::Value::Object(args) }
    }));
    assert!(
        follow.get("error").is_none(),
        "continuation must not raise a JSON-RPC error: {follow}"
    );
    let follow_text = extract_tool_text_str(&follow);
    assert!(
        !follow_text.contains("ERROR:"),
        "continuation should succeed (no error line protocol), got:\n{follow_text}"
    );
}

#[test]
fn jobs_wait_mode_watch_terminal_renders_single_line() {
    let mut server = Server::start_initialized("jobs_wait_mode_watch_terminal_renders_single_line");
    let ws = "ws_jobs_wait_watch_done";
    let job_id = create_job(&mut server, ws);

    // Claim (QUEUED -> RUNNING).
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
                "args": { "job": job_id.clone(), "runner_id": "runner_1" }
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

    // Complete (DONE).
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
                "args": { "job": job_id.clone(), "runner_id": "runner_1", "claim_revision": claim_revision, "status": "DONE" }
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
                "args": { "job": job_id, "timeout_ms": 0, "mode": "watch" }
            }
        }
    }));
    let rendered = extract_tool_text_str(&wait);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "terminal job should render 1 line (no continuation), got:\n{rendered}"
    );
    assert!(
        lines[0].contains("DONE") && lines[0].contains("done=true"),
        "expected DONE terminal line, got:\n{}",
        lines[0]
    );
}
