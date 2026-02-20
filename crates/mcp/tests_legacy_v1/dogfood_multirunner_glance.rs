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
fn jobs_radar_is_glanceable_with_multirunner_and_needs_manager() {
    let mut server =
        Server::start_initialized("jobs_radar_is_glanceable_with_multirunner_and_needs_manager");

    // Create two jobs: one RUNNING (needs manager), one QUEUED.
    let created1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Job A", "prompt": "do A" } } }
    }));
    let job1 = job_id_from_create(&created1);

    let created2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Job B", "prompt": "do B" } } }
    }));
    let job2 = job_id_from_create(&created2);

    let claimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": { "workspace": "ws1", "job": job1, "runner_id": "r-live", "lease_ttl_ms": 5000 } } }
    }));
    let claim_rev = claim_revision_from_claim(&claimed);

    // Make it explicit that the agent needs a manager decision (hunt-free).
    let _q = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job1, "runner_id": "r-live", "claim_revision": claim_rev, "lease_ttl_ms": 5000, "kind": "question", "message": "Need decision", "refs": [job1] } } }
    }));

    // Two runners: one live on job1, one idle.
    let _hb_live = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-live", "status": "live", "active_job_id": job1, "lease_ttl_ms": 5000 } } }
    }));
    let _hb_idle = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r-idle", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));

    // Glanceable inbox: all actionable lines should be readable without opening.
    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 10, "runners_limit": 10 } } }
    }));
    let text = extract_tool_text_str(&radar);

    let header = text.lines().next().unwrap_or("");
    assert!(
        header.contains("jobs_radar")
            && header.contains("runner=")
            && header.contains("runners=live:")
            && header.contains("idle:")
            && !header.contains("offline:"),
        "expected runner summary in header, got:\n{text}"
    );

    // With a live runner present, we should not spam the runner bootstrap hint.
    assert!(
        !text.lines().any(|l| l.starts_with("CMD: ")),
        "did not expect runner bootstrap hint when a runner is already live/idle, got:\n{text}"
    );

    // Runner lines show who is live and on what job.
    assert!(
        text.lines().any(|l| l.contains("runner live")
            && l.contains("r-live")
            && l.contains("job=")
            && l.contains("open id=runner:r-live")),
        "expected live runner line, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.contains("runner idle")
            && l.contains("r-idle")
            && l.contains("open id=runner:r-idle")),
        "expected idle runner line, got:\n{text}"
    );

    // Job lines must be ref-first and include an open action.
    assert!(
        text.lines()
            .any(|l| l.contains(&format!(" {job1} (RUNNING)"))
                && l.contains("| open id=")
                && l.contains("| reply reply_job=")),
        "expected RUNNING job line with reply hint, got:\n{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains(&format!(" {job2} (QUEUED)")) && l.contains("| open id=")),
        "expected QUEUED job line, got:\n{text}"
    );
}

#[test]
fn jobs_radar_includes_runner_bootstrap_hint_only_when_offline_and_queued() {
    let mut server = Server::start_initialized(
        "jobs_radar_includes_runner_bootstrap_hint_only_when_offline_and_queued",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Queued job", "prompt": "do it" } } }
    }));
    let job_id = job_id_from_create(&created);
    assert!(job_id.starts_with("JOB-"));

    // Offline (no runner lease) + queued => show the copy/paste runner bootstrap hint.
    let radar0 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text0 = extract_tool_text_str(&radar0);
    assert!(
        text0.lines().any(|l| l.starts_with("CMD: ")),
        "expected runner bootstrap hint when offline+queued, got:\n{text0}"
    );

    // Once a runner heartbeats (idle), the hint should disappear (no noise).
    let _hb = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r1", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));
    let radar1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let text1 = extract_tool_text_str(&radar1);
    assert!(
        !text1.lines().any(|l| l.starts_with("CMD: ")),
        "did not expect bootstrap hint when runner is present, got:\n{text1}"
    );
}

#[test]
fn tasks_snapshot_surfaces_inbox_runner_status_and_bootstrap_hint() {
    let mut server =
        Server::start_initialized("tasks_snapshot_surfaces_inbox_runner_status_and_bootstrap_hint");

    // Ensure the workspace has at least one task so tasks_snapshot has a focus/target.
    let task_started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "workspace": "ws1", "task_title": "Main task" } } }
    }));
    let task_text = extract_tool_text_str(&task_started);
    let task_id = task_text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("TASK-001")
        .to_string();

    // Create a queued job so the snapshot can surface an explicit inbox status.
    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Queued job", "prompt": "do it" } } }
    }));
    let job_id = job_id_from_create(&created);
    assert!(job_id.starts_with("JOB-"));

    // Offline + queued => show runner=offline and a copy/paste runner bootstrap CMD.
    let snap0 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "workspace": "ws1", "task": task_id, "fmt": "lines" } } }
    }));
    let text0 = extract_tool_text_str(&snap0);
    let first0 = text0.lines().next().unwrap_or("");
    assert!(
        first0.contains("inbox running=")
            && first0.contains("queued=")
            && first0.contains("runner=offline"),
        "expected inbox+runner summary in snapshot state line, got:\n{text0}"
    );
    assert!(
        text0.lines().any(|l| l.starts_with("CMD: ")),
        "expected runner bootstrap CMD hint in snapshot when offline+queued, got:\n{text0}"
    );

    // Once a runner heartbeats, the CMD hint should disappear (no noise), but runner status stays explicit.
    let _hb = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.runner.heartbeat", "args": { "workspace": "ws1", "runner_id": "r1", "status": "idle", "lease_ttl_ms": 5000 } } }
    }));
    let snap1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "workspace": "ws1", "task": task_id, "fmt": "lines" } } }
    }));
    let text1 = extract_tool_text_str(&snap1);
    let first1 = text1.lines().next().unwrap_or("");
    assert!(
        first1.contains("runner=idle") || first1.contains("runner=live"),
        "expected explicit runner status in snapshot, got:\n{text1}"
    );
    assert!(
        !text1.lines().any(|l| l.starts_with("CMD: ")),
        "did not expect runner bootstrap CMD once a runner is present, got:\n{text1}"
    );
}
