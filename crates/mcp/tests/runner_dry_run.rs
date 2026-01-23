#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_runner_{test_name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn workspace_root() -> PathBuf {
    // crates/mcp/tests/... => crates/mcp => crates => workspace
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .expect("workspace root")
}

#[test]
fn bm_runner_dry_run_claims_and_completes_a_job() {
    // 1) Create a job in a fresh store.
    let storage_dir = temp_dir("bm_runner_dry_run_claims_and_completes_a_job");
    let job_id: String = {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--toolset", "full"], false);
        server.initialize_default();

        let created = server.request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "tasks_jobs_create",
                "arguments": {
                    "workspace": "ws_runner",
                    "title": "Runner smoke: dry-run",
                    "prompt": "This is a test job. The runner should claim and complete it in dry-run mode.",
                    "kind": "codex_cli",
                    "priority": "LOW"
                }
            }
        }));

        let parsed = extract_tool_text(&created);
        parsed
            .get("result")
            .and_then(|v| v.get("job"))
            .and_then(|v| v.get("job_id"))
            .and_then(|v| v.as_str())
            .expect("job.job_id")
            .to_string()
    };

    // 2) Ensure bm_runner binary exists (debug build) and run it in dry-run mode.
    let root = workspace_root();
    let status = Command::new("cargo")
        .current_dir(&root)
        .args(["build", "-p", "bm_runner"])
        .status()
        .expect("cargo build bm_runner");
    assert!(status.success(), "bm_runner must build");

    let runner_path = root.join("target").join("debug").join("bm_runner");
    let status = Command::new(&runner_path)
        .env("BRANCHMIND_VIEWER", "0")
        .args([
            "--storage-dir",
            storage_dir.to_str().expect("storage dir utf-8"),
            "--workspace",
            "ws_runner",
            "--mcp-bin",
            env!("CARGO_BIN_EXE_bm_mcp"),
            "--dry-run",
            "--once",
        ])
        .status()
        .expect("run bm_runner");
    assert!(status.success(), "bm_runner must exit 0");

    // 3) Re-open the job and confirm it is DONE.
    let mut server =
        Server::start_with_storage_dir(storage_dir.clone(), &["--toolset", "full"], true);
    server.initialize_default();

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "tasks_jobs_open",
            "arguments": {
                "workspace": "ws_runner",
                "job": job_id,
                "include_prompt": false,
                "include_events": true,
                "max_events": 5,
                "max_chars": 4000
            }
        }
    }));
    let parsed = extract_tool_text(&opened);
    let status = parsed
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(status, "DONE", "expected job status DONE, got {status}");
}
