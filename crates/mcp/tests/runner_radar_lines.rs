#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn jobs_radar_fmt_lines_includes_bounded_runner_lines() {
    let mut server =
        Server::start_initialized("jobs_radar_fmt_lines_includes_bounded_runner_lines");

    // Two active leases (live + idle).
    let _hb1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-live", "status": "live", "active_job_id": "JOB-001", "lease_ttl_ms": 5000 } } }
    }));
    let _hb2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-idle", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text = extract_tool_text_str(&radar_lines);
    let header = text.lines().next().unwrap_or("");
    assert!(
        header.contains("runner=")
            && header.contains("runners=live:")
            && header.contains("idle:")
            && !header.contains("offline:"),
        "expected runner summary in header, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| {
            l.contains("runner live")
                && l.contains("r-live")
                && l.contains("job=JOB-001")
                && l.contains("open id=runner:r-live")
        }),
        "expected live runner line with active job, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.contains("runner idle")
            && l.contains("r-idle")
            && l.contains("open id=runner:r-idle")),
        "expected idle runner line, got:\n{text}"
    );
}

#[test]
fn jobs_radar_fmt_lines_includes_recent_offline_runners_section() {
    let mut server =
        Server::start_initialized("jobs_radar_fmt_lines_includes_recent_offline_runners_section");

    // Create a lease that will expire soon, so we can observe the explicit offline runner section.
    let _hb = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-offline", "status": "idle", "lease_ttl_ms": 2000 } } }
    }));

    // Heartbeat TTL is intentionally "sleep-safe" to avoid flapping, so we wait past expiry.
    sleep(Duration::from_millis(2200));

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5, "offline_limit": 5 } } }
    }));
    let text = extract_tool_text_str(&radar_lines);

    assert!(
        text.lines().any(|l| l.contains("runner offline")
            && l.contains("r-offline")
            && l.contains("open id=runner:r-offline")),
        "expected an explicit offline runner line with openable runner ref, got:\n{text}"
    );
}

#[test]
fn jobs_radar_fmt_lines_marks_running_job_runner_offline_when_no_lease() {
    let mut server = Server::start_initialized(
        "jobs_radar_fmt_lines_marks_running_job_runner_offline_when_no_lease",
    );

    // Create and claim a job (RUNNING) without starting a runner heartbeat lease.
    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Job A", "prompt": "do it" } } }
    }));
    let job_id = extract_tool_text(&created)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let _claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r-offline", "lease_ttl_ms": 5000 } } }
    }));

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text = extract_tool_text_str(&radar_lines);
    assert!(
        text.lines().any(|l| l.contains("runner=offline:r-offline")),
        "expected running job line to mark runner as offline when no lease exists, got:\n{text}"
    );
}

#[test]
fn jobs_radar_fmt_lines_keeps_running_job_runner_state_explicit_when_runner_leases_incomplete() {
    let mut server = Server::start_initialized(
        "jobs_radar_fmt_lines_keeps_running_job_runner_state_explicit_when_runner_leases_incomplete",
    );

    // Make the runner lease set intentionally incomplete (has_more=true) by creating multiple
    // runner leases but requesting a very small runners_limit.
    let _hb1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-a", "status": "live", "lease_ttl_ms": 5000 } } }
    }));
    let _hb2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-b", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));
    let _hb3 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-unknown", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));

    // Create and claim a job (RUNNING) with a runner id that has an active lease, but is not
    // guaranteed to appear in the truncated runners list (runners_limit=1).
    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Job B", "prompt": "do it" } } }
    }));
    let job_id = extract_tool_text(&created)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job.job_id")
        .to_string();

    let _claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r-unknown", "lease_ttl_ms": 5000 } } }
    }));

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5, "runners_limit": 1 } } }
    }));
    let text = extract_tool_text_str(&radar_lines);
    assert!(
        text.lines().any(|l| l.contains("runner=idle:r-unknown")),
        "expected running job line to keep runner state explicit even when leases list is incomplete, got:\n{text}"
    );
}
