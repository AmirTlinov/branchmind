#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn jobs_proof_attach_creates_evidence_from_job_refs() {
    let mut server = Server::start_initialized_with_args(
        "jobs_proof_attach_creates_evidence_from_job_refs",
        &["--toolset", "full", "--workspace", "ws_jobs_proof_attach"],
    );

    // Prepare a local artifact to verify sha256 hashing.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let artifact_path = std::env::temp_dir().join(format!(
        "bm_mcp_jobs_proof_attach_artifact_{}_{}.txt",
        std::process::id(),
        nonce
    ));
    std::fs::write(&artifact_path, b"hello").expect("write artifact");
    let expected_sha256 = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    // Create a task to set focus for evidence capture.
    let _started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
            "workspace": "ws_jobs_proof_attach",
            "task_title": "Job Proof Attach",
            "template": "basic-task"
        } } }
    }));

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
            "workspace": "ws_jobs_proof_attach",
            "title": "Proof Job",
            "prompt": "noop"
        } } }
    }));
    let created_out = extract_tool_text(&created);
    assert!(
        created_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.create must succeed: {created_out}"
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
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": {
            "workspace": "ws_jobs_proof_attach",
            "job": job_id,
            "runner_id": "r1"
        } } }
    }));
    let claimed_out = extract_tool_text(&claimed);
    assert!(
        claimed_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.claim must succeed: {claimed_out}"
    );
    let claim_revision = claimed_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("job.revision claim token");

    let completed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": {
            "workspace": "ws_jobs_proof_attach",
            "job": job_id,
            "runner_id": "r1",
            "claim_revision": claim_revision,
            "status": "DONE",
            "summary": "CMD: cargo test -q\nLINK: https://example.com/log"
        } } }
    }));
    let completed_out = extract_tool_text(&completed);
    assert!(
        completed_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.complete must succeed: {completed_out}"
    );

    let attached = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.proof.attach", "args": {
            "workspace": "ws_jobs_proof_attach",
            "job": job_id,
            "artifact_ref": artifact_path.to_string_lossy()
        } } }
    }));
    let attached_out = extract_tool_text(&attached);
    assert!(
        attached_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.proof.attach must succeed: {attached_out}"
    );
    assert!(
        attached_out
            .get("result")
            .and_then(|v| v.get("event"))
            .is_some(),
        "jobs.proof.attach should return an evidence event"
    );

    let files = attached_out
        .get("result")
        .and_then(|v| v.get("files"))
        .and_then(|v| v.as_array())
        .expect("jobs.proof.attach should return files when artifact_ref points to a local file");
    assert!(
        files.iter().any(|f| {
            f.get("sha256").and_then(|v| v.as_str()) == Some(expected_sha256)
                && f.get("uri")
                    .and_then(|v| v.as_str())
                    .is_some_and(|u| u.starts_with("file://"))
        }),
        "expected a file hash entry (sha256 + file:// uri): {files:?}"
    );

    let _ = std::fs::remove_file(&artifact_path);
}
