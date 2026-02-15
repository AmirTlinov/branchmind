#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_runner_defaults_{test_name}_{pid}_{nonce}"));
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

fn normalize_workspace_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

fn repo_workspace_id(root: &Path) -> String {
    normalize_workspace_id(
        root.file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("workspace"),
    )
}

#[test]
fn bm_runner_defaults_can_find_repo_root_store_from_subdir() {
    let repo_root = temp_dir("repo_root_from_subdir");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create fake .git");
    let nested = repo_root.join("a").join("b");
    std::fs::create_dir_all(&nested).expect("create nested dir");

    let storage_dir = repo_root.join(".agents").join("mcp").join(".branchmind");
    let workspace = repo_workspace_id(&repo_root);

    // 1) Create a job in the repo-root store/workspace.
    let job_id: String = {
        let mut server =
            Server::start_with_storage_dir(storage_dir.clone(), &["--toolset", "full"], false);
        server.initialize_default();

        let created = server.request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                    "workspace": workspace,
                    "title": "Runner defaults smoke",
                    "prompt": "dry-run"
                } } }
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

    // 2) Ensure bm_runner binary exists (debug build) and run it from a subdirectory
    // without passing --workspace/--storage-dir/--mcp-bin (defaults must match bm_mcp).
    let root = workspace_root();
    let status = Command::new("cargo")
        .current_dir(&root)
        .args(["build", "-p", "bm_runner"])
        .status()
        .expect("cargo build bm_runner");
    assert!(status.success(), "bm_runner must build");

    let runner_path = root.join("target").join("debug").join("bm_runner");
    let status = Command::new(&runner_path)
        .current_dir(&nested)
        .env("BRANCHMIND_VIEWER", "0")
        // CI machines do not have codex/claude installed. `--dry-run` must still succeed and
        // complete the claimed job.
        .env("BM_CODEX_BIN", "__missing_codex__")
        .env("BM_CLAUDE_BIN", "__missing_claude__")
        .args(["--dry-run", "--once"])
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
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": repo_workspace_id(&repo_root),
                "job": job_id,
                "include_prompt": false,
                "include_events": true,
                "max_events": 5,
                "max_chars": 4000
            } } }
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
