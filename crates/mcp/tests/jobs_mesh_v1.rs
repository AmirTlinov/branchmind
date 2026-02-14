#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use sha2::Digest as _;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[test]
fn jobs_mesh_publish_validates_code_ref_and_supports_pull_ack() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();

    let mut server = Server::start_initialized_with_args(
        "jobs_mesh_publish_validates_code_ref_and_supports_pull_ack",
        &["--agent-id", "manager"],
    );

    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref = format!("code:README.md#L1-L3@sha256:{sha}");

    let publish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.mesh.publish",
                "args": {
                    "thread_id": "workspace/main",
                    "kind": "message",
                    "summary": "hello",
                    "refs": [code_ref],
                    "idempotency_key": "idem1",
                    "from_agent_id": "alice"
                }
            }
        }
    }));
    let publish_text = extract_tool_text(&publish);
    assert_eq!(
        publish_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "publish should succeed: {publish_text}"
    );
    let warnings = publish_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !warnings
            .iter()
            .any(|w| w.get("code").and_then(|v| v.as_str()) == Some("CODE_REF_STALE")),
        "valid CODE_REF must not be stale: {warnings:?}"
    );

    let pull = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.mesh.pull",
                "args": {
                    "thread_id": "workspace/main",
                    "consumer_id": "manager",
                    "limit": 50
                }
            }
        }
    }));
    let pull_text = extract_tool_text(&pull);
    assert_eq!(
        pull_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "pull should succeed: {pull_text}"
    );
    let messages = pull_text
        .get("result")
        .and_then(|v| v.get("messages"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !messages.is_empty(),
        "pull should return messages: {pull_text}"
    );
    let next_after = pull_text
        .get("result")
        .and_then(|v| v.get("next_after_seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(next_after > 0, "next_after_seq should advance");

    let ack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.mesh.ack",
                "args": {
                    "thread_id": "workspace/main",
                    "consumer_id": "manager",
                    "after_seq": next_after
                }
            }
        }
    }));
    let ack_text = extract_tool_text(&ack);
    assert_eq!(
        ack_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "ack should succeed: {ack_text}"
    );

    let pull2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.mesh.pull",
                "args": {
                    "thread_id": "workspace/main",
                    "consumer_id": "manager",
                    "limit": 50
                }
            }
        }
    }));
    let pull2_text = extract_tool_text(&pull2);
    assert_eq!(
        pull2_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "pull2 should succeed: {pull2_text}"
    );
    let messages2 = pull2_text
        .get("result")
        .and_then(|v| v.get("messages"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        messages2.is_empty(),
        "after ack, pull should be empty: {pull2_text}"
    );
}

#[test]
fn jobs_control_center_includes_team_mesh_threads() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();

    let mut server = Server::start_initialized_with_args(
        "jobs_control_center_includes_team_mesh_threads",
        &["--agent-id", "manager"],
    );

    // Seed a mesh message so unread_count is non-zero.
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.mesh.publish",
                "args": {
                    "thread_id": "workspace/main",
                    "kind": "message",
                    "summary": "seed",
                    "idempotency_key": "idem_seed",
                    "from_agent_id": "alice"
                }
            }
        }
    }));
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.sync.team",
                "args": {
                    "task": "TASK-MESH",
                    "plan_delta": { "step": "seed" },
                    "idempotency_key": "idem_seed_task_thread"
                }
            }
        }
    }));

    let center = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.control.center",
                "args": { "limit": 5, "max_chars": 8000 }
            }
        }
    }));

    let center_text = extract_tool_text(&center);
    assert_eq!(
        center_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "control.center should succeed: {center_text}"
    );
    let enabled = center_text
        .get("result")
        .and_then(|v| v.get("team_mesh"))
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(enabled, "team_mesh.enabled must be true");
    let threads = center_text
        .get("result")
        .and_then(|v| v.get("team_mesh"))
        .and_then(|v| v.get("threads"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        threads
            .iter()
            .any(|t| t.get("thread_id").and_then(|v| v.as_str()) == Some("workspace/main")),
        "threads must include workspace/main: {threads:?}"
    );
    assert!(
        threads
            .iter()
            .any(|t| t.get("thread_id").and_then(|v| v.as_str()) == Some("task/TASK-MESH")),
        "threads must include task thread discovered from recent mesh activity: {threads:?}"
    );
    let actions = center_text
        .get("result")
        .and_then(|v| v.get("actions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("cmd").and_then(|v| v.as_str()) == Some("jobs.mesh.pull")
                && a.get("args")
                    .and_then(|v| v.get("thread_id"))
                    .and_then(|v| v.as_str())
                    .is_some()
        }),
        "with unread threads, control.center should suggest jobs.mesh.pull actions: {actions:?}"
    );

    // Ack both known threads and verify no unread-based pull actions remain.
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.mesh.ack",
                "args": { "thread_id": "workspace/main", "after_seq": 999 }
            }
        }
    }));
    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.mesh.ack",
                "args": { "thread_id": "task/TASK-MESH", "after_seq": 999 }
            }
        }
    }));
    let center2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "call",
                "cmd": "jobs.control.center",
                "args": { "limit": 5, "max_chars": 8000 }
            }
        }
    }));
    let center2_text = extract_tool_text(&center2);
    let actions2 = center2_text
        .get("result")
        .and_then(|v| v.get("actions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions2
            .iter()
            .all(|a| a.get("cmd").and_then(|v| v.as_str()) != Some("jobs.mesh.pull")),
        "after ack/unread=0, control.center should not suggest mesh.pull actions: {actions2:?}"
    );
}
