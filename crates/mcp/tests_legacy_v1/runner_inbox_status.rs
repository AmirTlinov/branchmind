#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

fn runner_status(resp: &serde_json::Value) -> String {
    resp.get("result")
        .and_then(|v| v.get("runner_status"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("-")
        .to_string()
}

#[test]
fn jobs_radar_surfaces_explicit_runner_status_offline_idle_live() {
    let mut server =
        Server::start_initialized("jobs_radar_surfaces_explicit_runner_status_offline_idle_live");

    // Offline by default (no lease written).
    let radar0 = extract_tool_text(&server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 5 } } }
    })));
    assert_eq!(runner_status(&radar0), "offline");

    // Idle lease makes inbox unambiguous.
    let _hb_idle = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r1", "status": "idle", "lease_ttl_ms": 2000 } } }
    }));
    let radar1 = extract_tool_text(&server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 5 } } }
    })));
    assert_eq!(runner_status(&radar1), "idle");

    // Live lease is explicit (not inferred from job events).
    let _hb_live = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r1", "status": "live", "active_job_id": "JOB-001", "lease_ttl_ms": 2000 } } }
    }));
    let radar2 = extract_tool_text(&server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 5 } } }
    })));
    assert_eq!(runner_status(&radar2), "live");

    // Lease expiry => offline again (no heuristics).
    sleep(Duration::from_millis(2200));
    let radar3 = extract_tool_text(&server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 5 } } }
    })));
    assert_eq!(runner_status(&radar3), "offline");
}
