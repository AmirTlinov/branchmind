#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

#[test]
fn runner_autostart_suppresses_bootstrap_hint_when_runner_bin_is_present() {
    // Ensure there is a `bm_runner` executable next to `bm_mcp` so the autostart path is stable
    // in CI (where the runner binary may not be built as part of bm_mcp tests).
    //
    // The real runner is validated in bm_runner's own crate; here we only assert the bm_mcp
    // autostart UX behavior.
    let mcp_bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_bm_mcp"));
    if let Some(dir) = mcp_bin.parent() {
        let runner = dir.join("bm_runner");
        if !runner.exists() {
            std::fs::write(&runner, "#!/usr/bin/env sh\nexit 0\n").expect("write fake bm_runner");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&runner)
                    .expect("stat fake bm_runner")
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&runner, perms).expect("chmod fake bm_runner");
            }
        }
    }

    let mut server = Server::start_with_args(
        "runner_autostart_suppresses_bootstrap_hint_when_runner_bin_is_present",
        &[
            "--toolset",
            "daily",
            "--runner-autostart",
            "--runner-autostart-dry-run",
        ],
    );

    let _created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Queued job", "prompt": "do it" } } }
    }));

    // Offline + queued should trigger an autostart attempt, suppressing the manual bootstrap CMD.
    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text = extract_tool_text_str(&radar);
    assert!(
        !text.lines().any(|l| l.starts_with("CMD: ")),
        "did not expect runner bootstrap hint when autostart is enabled, got:\n{text}"
    );

    // In a real system the runner would heartbeat shortly after spawning. Simulate that to ensure
    // the view transitions away from runner=offline without manual intervention.
    let _hb = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-auto", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));
    let radar1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text1 = extract_tool_text_str(&radar1);
    let header1 = text1.lines().next().unwrap_or("");
    assert!(
        header1.contains("runner=idle") || header1.contains("runner=live"),
        "expected runner to be non-offline after heartbeat, got:\n{text1}"
    );
}
