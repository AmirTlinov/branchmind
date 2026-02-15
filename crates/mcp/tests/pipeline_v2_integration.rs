#![forbid(unsafe_code)]

//! Integration tests for Pipeline v2 commands:
//! - jobs.macro.dispatch.writer
//! - jobs.pipeline.pre_validate
//! - jobs.pipeline.cascade.init
//! - jobs.pipeline.cascade.advance

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

// ── Helper: create a plan + task + anchor for pipeline tests ──

fn setup_plan_slice_and_anchor(server: &mut Server, ws: &str) -> (String, String, String) {
    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": ws,
            "kind": "plan",
            "title": "Plan Pipeline V2"
        } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    assert!(
        created_plan_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks.plan.create must succeed: {created_plan_text}"
    );
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let proposed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.propose_next", "args": {
            "workspace": ws,
            "plan": &plan_id,
            "objective": "Add rate limiter middleware to API",
            "constraints": ["no breaking changes"],
            "policy": "fail_closed"
        } } }
    }));
    let proposed_text = extract_tool_text(&proposed);
    assert!(
        proposed_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks.slices.propose_next must succeed: {proposed_text}"
    );
    let slice_plan_spec = proposed_text
        .get("result")
        .and_then(|v| v.get("slice_plan_spec"))
        .cloned()
        .expect("slice_plan_spec");

    let applied = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.apply", "args": {
            "workspace": ws,
            "plan": &plan_id,
            "slice_plan_spec": slice_plan_spec,
            "policy": "fail_closed"
        } } }
    }));
    let applied_text = extract_tool_text(&applied);
    assert!(
        applied_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks.slices.apply must succeed: {applied_text}"
    );
    let slice_id = applied_text
        .get("result")
        .and_then(|v| v.get("slice"))
        .and_then(|v| v.get("slice_id"))
        .and_then(|v| v.as_str())
        .expect("slice_id")
        .to_string();

    let anchor_id = format!("a:{}", slice_id.to_ascii_lowercase());
    (plan_id, slice_id, anchor_id)
}

/// Dispatch scout, claim, complete with a strict scout_context_pack summary.
/// Returns (scout_job_id, artifact_ref).
fn dispatch_scout_and_complete(
    server: &mut Server,
    ws: &str,
    plan_id: &str,
    anchor_id: &str,
    slice_id: &str,
) -> (String, String) {
    let dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.macro.dispatch.scout", "args": {
            "workspace": ws,
            "task": plan_id,
            "anchor": anchor_id,
            "slice_id": slice_id,
            "objective": "Add rate limiter middleware to API",
            "constraints": ["no breaking changes"],
            "quality_profile": "standard"
        } } }
    }));
    let dispatch_text = extract_tool_text(&dispatch);
    assert!(
        dispatch_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "dispatch_scout must succeed: {dispatch_text}"
    );
    let job_id = dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout result.job.job_id")
        .to_string();

    let revision = claim_job(server, ws, &job_id, "runner-1", None, false);

    let root = repo_root();
    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L3@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L4-L8@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L9-L14@sha256:{sha}");

    let scout_pack = json!({
        "format_version": 1,
        "objective": "Add rate limiter middleware to API",
        "scope": { "in": ["README.md"], "out": ["tests/"] },
        "code_refs": [code_ref_a, code_ref_b, code_ref_c],
        "change_hints": [
            { "path": "README.md", "intent": "Document rate-limiter rollout", "risk": "low" },
            { "path": "README.md", "intent": "Document middleware contract", "risk": "medium" }
        ],
        "anchors": [
            {
                "id": anchor_id,
                "anchor_type": "primary",
                "rationale": "Primary rollout anchor",
                "code_ref": format!("code:README.md#L1-L3@sha256:{sha}"),
                "content": "BranchMind — Unified task + reasoning",
                "line_count": 3
            },
            {
                "id": "a:pipeline",
                "anchor_type": "dependency",
                "rationale": "Pipeline contract dependency",
                "code_ref": format!("code:README.md#L4-L8@sha256:{sha}"),
                "content": "Mission and principles",
                "line_count": 5
            },
            {
                "id": "a:docs",
                "anchor_type": "structural",
                "rationale": "Docs structure anchor",
                "code_ref": format!("code:README.md#L9-L14@sha256:{sha}"),
                "content": "Repository structure overview",
                "line_count": 6
            }
        ],
        "test_hints": [
            "cargo test -p bm_mcp --test pipeline_v2_integration",
            "cargo test -p bm_mcp jobs_ai_first_ux"
        ],
        "risk_map": [
            { "risk": "regression in command routing", "falsifier": "run pipeline_v2 integration suite" },
            { "risk": "lineage drift in artifacts", "falsifier": "run jobs.pipeline.gate/apply smoke" }
        ]
        ,
        "open_questions": [],
        "summary_for_builder": "Scout context confirms README-based anchors and explicit CODE_REF lineage for pipeline v2 integration tests. Builder should preserve fail-closed contracts for dispatch/gate/apply flows, keep deterministic command routing, and avoid alias regressions. Primary risk is silent contract drift between test fixtures and strict schemas; falsify by running pipeline_v2 integration plus jobs_ai_first_ux gate/apply flows."
    });

    let complete = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": {
            "workspace": ws,
            "job": &job_id,
            "runner_id": "runner-1",
            "claim_revision": revision,
            "status": "DONE",
            "summary": serde_json::to_string(&scout_pack).unwrap()
        } } }
    }));
    let complete_text = extract_tool_text(&complete);
    assert!(
        complete_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "jobs.complete must succeed: {complete_text}"
    );

    let artifact_ref = format!("artifact://jobs/{job_id}/scout_context_pack");
    (job_id, artifact_ref)
}

// ── Test: dispatch_writer creates a writer job ──

#[test]
fn pipeline_v2_dispatch_writer_creates_job() {
    let mut server = Server::start_initialized("pipeline_v2_dispatch_writer_creates_job");
    let ws = "ws_pipe_v2_writer";
    let (plan_id, slice_id, anchor_id) = setup_plan_slice_and_anchor(&mut server, ws);
    let (_scout_job_id, scout_ref) =
        dispatch_scout_and_complete(&mut server, ws, &plan_id, &anchor_id, &slice_id);

    let writer_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.macro.dispatch.writer", "args": {
            "workspace": ws,
            "task": &plan_id,
            "slice_id": &slice_id,
            "scout_pack_ref": &scout_ref,
            "objective": "Add rate limiter middleware to API",
            "dod": {
                "criteria": ["writer patch pack references scout scope only"],
                "tests": ["pipeline_v2_integration"],
                "security": ["no path traversal in patch ops"]
            }
        } } }
    }));
    let writer_text = extract_tool_text(&writer_resp);
    assert!(
        writer_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "dispatch_writer must succeed: {writer_text}"
    );

    let writer_job_id = writer_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str());
    assert!(
        writer_job_id.is_some_and(|id| id.starts_with("JOB-")),
        "dispatch_writer must return a JOB-* id in result.job.job_id: {writer_text}"
    );

    // Verify the writer job exists in jobs list.
    let list_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.list", "args": {
            "workspace": ws,
            "status": "QUEUED",
            "task": &plan_id,
            "limit": 10
        } } }
    }));
    let list_text = extract_tool_text(&list_resp);
    let jobs = list_text
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        jobs.iter().any(|j| j
            .get("job_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == writer_job_id.unwrap())),
        "writer job must appear in jobs list"
    );
}

// ── Test: pre_validate on a valid scout pack ──

#[test]
fn pipeline_v2_pre_validate_on_completed_scout() {
    let mut server = Server::start_initialized("pipeline_v2_pre_validate_on_completed_scout");
    let ws = "ws_pipe_v2_preval";
    let (plan_id, slice_id, anchor_id) = setup_plan_slice_and_anchor(&mut server, ws);
    let (_scout_job_id, scout_ref) =
        dispatch_scout_and_complete(&mut server, ws, &plan_id, &anchor_id, &slice_id);

    let preval_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.pipeline.pre_validate", "args": {
            "workspace": ws,
            "scout_pack_ref": &scout_ref
        } } }
    }));
    let preval_text = extract_tool_text(&preval_resp);
    assert!(
        preval_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "pre_validate must succeed: {preval_text}"
    );

    let verdict = preval_text.get("result").and_then(|v| v.get("verdict"));
    assert!(verdict.is_some(), "must return verdict: {preval_text}");

    let verdict_status = verdict
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str());
    assert!(
        verdict_status.is_some_and(|s| s == "pass" || s == "need_more" || s == "reject"),
        "verdict.status must be pass|need_more|reject: {preval_text}"
    );

    // Checks should be present.
    let checks = preval_text.get("result").and_then(|v| v.get("checks"));
    assert!(checks.is_some(), "must return checks: {preval_text}");
}

// ── Test: pre_validate rejects non-existent scout job ──

#[test]
fn pipeline_v2_pre_validate_rejects_bad_ref() {
    let mut server = Server::start_initialized("pipeline_v2_pre_validate_bad_ref");
    let ws = "ws_pipe_v2_preval_bad";

    let preval_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.pipeline.pre_validate", "args": {
            "workspace": ws,
            "scout_pack_ref": "artifact://jobs/JOB-nonexist/scout_context_pack"
        } } }
    }));
    let preval_text = extract_tool_text(&preval_resp);
    assert_eq!(
        preval_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "pre_validate with unknown job must fail: {preval_text}"
    );
}

// ── Test: cascade.init dispatches scout and returns session info ──

#[test]
fn pipeline_v2_cascade_init_dispatches_scout() {
    let mut server = Server::start_initialized("pipeline_v2_cascade_init");
    let ws = "ws_pipe_v2_cascade";
    let (plan_id, slice_id, anchor_id) = setup_plan_slice_and_anchor(&mut server, ws);

    let init_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 40,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.pipeline.cascade.init", "args": {
            "workspace": ws,
            "task": &plan_id,
            "anchor": &anchor_id,
            "slice_id": &slice_id,
            "objective": "Add rate limiter middleware to API"
        } } }
    }));
    let init_text = extract_tool_text(&init_resp);
    assert!(
        init_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "cascade.init must succeed: {init_text}"
    );

    let result = init_text.get("result").expect("result object");

    // Must return cascade_session_id.
    let session_id = result.get("cascade_session_id").and_then(|v| v.as_str());
    assert!(
        session_id.is_some_and(|id| id.starts_with("pls-")),
        "cascade.init must return cascade_session_id starting with pls-: {init_text}"
    );

    // Phase must be scout.
    let phase = result.get("phase").and_then(|v| v.as_str());
    assert_eq!(
        phase,
        Some("scout"),
        "initial phase must be scout: {init_text}"
    );

    // scout_dispatch must contain a nested success envelope with a job.
    let scout_dispatch = result.get("scout_dispatch").expect("scout_dispatch");
    let scout_success = scout_dispatch
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        scout_success,
        "scout_dispatch must succeed: {scout_dispatch}"
    );
    let scout_job_id = scout_dispatch
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str());
    assert!(
        scout_job_id.is_some_and(|id| id.starts_with("JOB-")),
        "scout_dispatch must contain JOB-* id: {scout_dispatch}"
    );
}

// ── Test: cascade.advance with scout_done event ──

#[test]
fn pipeline_v2_cascade_advance_after_scout_done() {
    let mut server = Server::start_initialized("pipeline_v2_cascade_advance");
    let ws = "ws_pipe_v2_cadv";
    let (plan_id, slice_id, anchor_id) = setup_plan_slice_and_anchor(&mut server, ws);

    // Init cascade — dispatches scout.
    let init_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 40,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.pipeline.cascade.init", "args": {
            "workspace": ws,
            "task": &plan_id,
            "anchor": &anchor_id,
            "slice_id": &slice_id,
            "objective": "Add rate limiter middleware to API"
        } } }
    }));
    let init_text = extract_tool_text(&init_resp);
    assert!(
        init_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "cascade.init must succeed: {init_text}"
    );

    let result = init_text.get("result").expect("result");
    let session_id = result
        .get("cascade_session_id")
        .and_then(|v| v.as_str())
        .expect("cascade_session_id")
        .to_string();
    let scout_job_id = result
        .get("scout_dispatch")
        .and_then(|v| v.get("result"))
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout_dispatch.result.job.job_id")
        .to_string();

    // Construct a CascadeSession JSON for advance (as it would be after scout phase).
    let session_json = json!({
        "session_id": session_id,
        "phase": "scout",
        "scout_retries": 0,
        "writer_retries": 0,
        "scout_reruns": 0,
        "total_llm_calls": 1,
        "lineage": {
            "scout_job_ids": [&scout_job_id],
            "writer_job_ids": [],
            "validator_job_ids": []
        }
    });

    // Advance cascade with scout_done event.
    let advance_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 43,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.pipeline.cascade.advance", "args": {
            "workspace": ws,
            "session_json": session_json,
            "event": "scout_done",
            "job_id": &scout_job_id
        } } }
    }));
    let advance_text = extract_tool_text(&advance_resp);
    assert!(
        advance_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "cascade.advance must succeed: {advance_text}"
    );

    // Phase should advance from scout -> pre_validate.
    let adv_result = advance_text.get("result").expect("result");
    let new_phase = adv_result.get("phase").and_then(|v| v.as_str());
    assert_eq!(
        new_phase,
        Some("pre_validate"),
        "phase must advance to pre_validate after scout_done: {advance_text}"
    );

    // Session should be returned.
    let session = adv_result.get("session");
    assert!(
        session.is_some(),
        "cascade.advance must return updated session: {advance_text}"
    );
}
