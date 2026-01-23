#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn job_id_from_create(resp: &serde_json::Value) -> String {
    extract_tool_text(resp)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job.job_id")
        .to_string()
}

fn claim_revision_from_claim(resp: &serde_json::Value) -> i64 {
    extract_tool_text(resp)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("job.revision")
}

#[test]
fn jobs_complete_salvages_proof_refs_from_summary_when_refs_missing() {
    let mut server = Server::start_initialized(
        "jobs_complete_salvages_proof_refs_from_summary_when_refs_missing",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_claim", "arguments": { "workspace": "ws1", "job": job, "runner_id": "r1", "lease_ttl_ms": 5000 } }
    }));
    let claim_rev = claim_revision_from_claim(&claimed);

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_complete", "arguments": {
            "workspace": "ws1",
            "job": job,
            "runner_id": "r1",
            "claim_revision": claim_rev,
            "status": "DONE",
            "summary": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n"
        } }
    }));

    let text = extract_tool_text(&done);
    let refs = text
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    assert!(
        refs.iter().any(|r| r.starts_with("JOB-")),
        "expected JOB-* navigation ref in completion event refs, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CMD: cargo test -q"),
        "expected salvaged CMD receipt, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r == "LINK: https://example.com/ci/run/123"),
        "expected salvaged LINK receipt, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CARD-7"),
        "expected salvaged CARD-* token, got: {refs:?}"
    );
}

#[test]
fn jobs_complete_salvages_proof_refs_even_when_refs_only_includes_job_id() {
    let mut server = Server::start_initialized(
        "jobs_complete_salvages_proof_refs_even_when_refs_only_includes_job_id",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_claim", "arguments": { "workspace": "ws1", "job": job, "runner_id": "r1", "lease_ttl_ms": 5000 } }
    }));
    let claim_rev = claim_revision_from_claim(&claimed);

    let done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_complete", "arguments": {
            "workspace": "ws1",
            "job": job,
            "runner_id": "r1",
            "claim_revision": claim_rev,
            "status": "DONE",
            "refs": [job],
            "summary": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n"
        } }
    }));

    let text = extract_tool_text(&done);
    let refs = text
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    assert!(
        refs.iter().any(|r| r == "CMD: cargo test -q"),
        "expected salvaged CMD receipt when refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r == "LINK: https://example.com/ci/run/123"),
        "expected salvaged LINK receipt when refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CARD-7"),
        "expected salvaged CARD-* token when refs only included job id, got: {refs:?}"
    );
}

#[test]
fn jobs_message_salvages_proof_refs_from_text_when_refs_missing() {
    let mut server =
        Server::start_initialized("jobs_message_salvages_proof_refs_from_text_when_refs_missing");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let msg = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_message", "arguments": {
            "workspace": "ws1",
            "job": job,
            "message": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n"
        } }
    }));

    let text = extract_tool_text(&msg);
    let refs = text
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    assert!(
        refs.iter().any(|r| r.starts_with("JOB-")),
        "expected JOB-* navigation ref in manager message refs, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CMD: cargo test -q"),
        "expected salvaged CMD receipt, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r == "LINK: https://example.com/ci/run/123"),
        "expected salvaged LINK receipt, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CARD-7"),
        "expected salvaged CARD-* token, got: {refs:?}"
    );
}

#[test]
fn jobs_message_salvages_proof_refs_even_when_refs_only_includes_job_id() {
    let mut server = Server::start_initialized(
        "jobs_message_salvages_proof_refs_even_when_refs_only_includes_job_id",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let msg = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_message", "arguments": {
            "workspace": "ws1",
            "job": job,
            "refs": [job],
            "message": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n"
        } }
    }));

    let text = extract_tool_text(&msg);
    let refs = text
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    assert!(
        refs.iter().any(|r| r == "CMD: cargo test -q"),
        "expected salvaged CMD receipt when refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r == "LINK: https://example.com/ci/run/123"),
        "expected salvaged LINK receipt when refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CARD-7"),
        "expected salvaged CARD-* token when refs only included job id, got: {refs:?}"
    );
}

#[test]
fn jobs_radar_reply_salvages_proof_refs_even_when_refs_only_includes_job_id() {
    let mut server = Server::start_initialized(
        "jobs_radar_reply_salvages_proof_refs_even_when_refs_only_includes_job_id",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    // Reply via jobs_radar shortcut (as a manager) with refs containing only the job id.
    let replied = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_radar", "arguments": {
            "workspace": "ws1",
            "reply_job": job,
            "reply_message": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n",
            "reply_refs": [job]
        } }
    }));

    let text = extract_tool_text(&replied);
    let refs = text
        .get("result")
        .and_then(|v| v.get("reply"))
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    assert!(
        refs.iter().any(|r| r == "CMD: cargo test -q"),
        "expected salvaged CMD receipt when reply_refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r == "LINK: https://example.com/ci/run/123"),
        "expected salvaged LINK receipt when reply_refs only included job id, got: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r == "CARD-7"),
        "expected salvaged CARD-* token when reply_refs only included job id, got: {refs:?}"
    );
}

#[test]
fn jobs_radar_marks_proof_gate_as_needs_proof_without_reply() {
    let mut server =
        Server::start_initialized("jobs_radar_marks_proof_gate_as_needs_proof_without_reply");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_claim", "arguments": { "workspace": "ws1", "job": job, "runner_id": "r-live", "lease_ttl_ms": 5000 } }
    }));
    let claim_rev = claim_revision_from_claim(&claimed);

    let _proof_gate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_report", "arguments": {
            "workspace": "ws1",
            "job": job,
            "runner_id": "r-live",
            "claim_revision": claim_rev,
            "lease_ttl_ms": 5000,
            "kind": "proof_gate",
            "message": "runner: proof gate: add proof refs",
            "refs": [job]
        } }
    }));

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_radar", "arguments": { "workspace": "ws1", "fmt": "lines", "limit": 10 } }
    }));
    let text = extract_tool_text_str(&radar);

    let job_line = text
        .lines()
        .find(|l| l.contains(&format!(" {job} (RUNNING)")))
        .unwrap_or("");
    assert!(
        !job_line.is_empty(),
        "expected a job line for {job}, got:\n{text}"
    );
    assert!(
        !job_line.contains("| reply reply_job="),
        "did not expect reply hint for proof_gate (needs_proof), got line:\n{job_line}"
    );
    assert!(
        job_line.contains("proof_gate:") || job_line.contains("proof gate"),
        "expected proof_gate preview for glanceability, got line:\n{job_line}"
    );
}

#[test]
fn jobs_radar_clears_needs_proof_after_manager_message_with_proof_refs() {
    let mut server = Server::start_initialized(
        "jobs_radar_clears_needs_proof_after_manager_message_with_proof_refs",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_create", "arguments": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } }
    }));
    let job = job_id_from_create(&created);

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_claim", "arguments": { "workspace": "ws1", "job": job, "runner_id": "r-live", "lease_ttl_ms": 5000 } }
    }));
    let claim_rev = claim_revision_from_claim(&claimed);

    let _proof_gate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_report", "arguments": {
            "workspace": "ws1",
            "job": job,
            "runner_id": "r-live",
            "claim_revision": claim_rev,
            "lease_ttl_ms": 5000,
            "kind": "proof_gate",
            "message": "runner: proof gate: add proof refs",
            "refs": [job]
        } }
    }));

    let radar_before = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_radar", "arguments": { "workspace": "ws1", "limit": 10 } }
    }));
    let before = extract_tool_text(&radar_before);
    let needs_proof_before = before
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .and_then(|jobs| {
            jobs.iter()
                .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job.as_str()))
        })
        .and_then(|j| j.get("attention"))
        .and_then(|a| a.get("needs_proof"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        needs_proof_before,
        "expected needs_proof before manager proof"
    );

    // Manager provides proof (but forgets refs); server salvages deterministic refs from text.
    let _msg = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_message", "arguments": {
            "workspace": "ws1",
            "job": job,
            "message": "- cargo test -q\n- https://example.com/ci/run/123\nCARD-7\n"
        } }
    }));

    let radar_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks_jobs_radar", "arguments": { "workspace": "ws1", "limit": 10 } }
    }));
    let after = extract_tool_text(&radar_after);
    let needs_proof_after = after
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .and_then(|jobs| {
            jobs.iter()
                .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job.as_str()))
        })
        .and_then(|j| j.get("attention"))
        .and_then(|a| a.get("needs_proof"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(
        !needs_proof_after,
        "expected needs_proof to clear after manager message includes proof refs"
    );
}
