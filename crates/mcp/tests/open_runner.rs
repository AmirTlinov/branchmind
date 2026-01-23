#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_runner_ref_is_supported_and_returns_lease_meta() {
    let mut server = Server::start_initialized_with_args(
        "open_runner_ref_is_supported_and_returns_lease_meta",
        &["--toolset", "daily", "--workspace", "ws_open_runner"],
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_open_runner" } }
    }));

    let _hb = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_runner_heartbeat", "arguments": {
            "workspace": "ws_open_runner",
            "runner_id": "r-open",
            "status": "live",
            "active_job_id": "JOB-001",
            "lease_ttl_ms": 5000,
            "meta": { "pid": 123, "client": "codex" }
        } }
    }));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": "runner:r-open" } }
    }));
    let opened = extract_tool_text(&opened);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(runner:*) should succeed"
    );
    let result = opened.get("result").unwrap_or(&serde_json::Value::Null);
    assert_eq!(result.get("kind").and_then(|v| v.as_str()), Some("runner"));
    assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("live"));

    let lease = result
        .get("lease")
        .and_then(|v| v.as_object())
        .expect("lease object");
    assert_eq!(
        lease.get("runner_id").and_then(|v| v.as_str()),
        Some("r-open"),
        "lease.runner_id should match"
    );
    assert!(
        lease
            .get("lease_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "lease should be active"
    );

    let meta = result.get("meta").unwrap_or(&serde_json::Value::Null);
    assert_eq!(meta.get("pid").and_then(|v| v.as_i64()), Some(123));
}
