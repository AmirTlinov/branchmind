#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

fn task_id_from_focus_line(text: &str) -> Option<String> {
    // v1 portals return structured JSON envelopes; most task macros surface `result.task_id`.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
        let result = parsed.get("result");
        let task_id = result
            .and_then(|v| v.get("task_id").or_else(|| v.get("task")))
            .and_then(|v| v.as_str())
            .or_else(|| result.and_then(|v| v.get("focus")).and_then(|v| v.as_str()));
        if let Some(id) = task_id
            && id.starts_with("TASK-")
        {
            return Some(id.to_string());
        }
    }

    let first = text.lines().next()?.trim();
    if !first.starts_with("focus ") {
        return None;
    }
    let mut parts = first.split_whitespace();
    let _focus = parts.next()?;
    let id = parts.next()?;
    if id.starts_with("TASK-") {
        Some(id.to_string())
    } else {
        None
    }
}

#[test]
fn tasks_macro_delegate_creates_job_and_snapshot_surfaces_it() {
    let mut server =
        Server::start_initialized("tasks_macro_delegate_creates_job_and_snapshot_surfaces_it");

    let delegate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.delegate", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Delegate Jobs",
                "task_title": "Storage: add delegation job HUD",
                "description": "Goal: ensure delegation is visible in tasks_snapshot without noisy logs.",
                "resume_max_chars": 4000
            } } }
    }));
    let out = extract_tool_text_str(&delegate);
    assert!(
        !out.starts_with("ERROR:"),
        "tasks_macro_delegate must succeed, got: {out}"
    );

    let task_id = task_id_from_focus_line(&out).expect("expected focus TASK-* line");

    let jobs_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.list", "args": {
                "workspace": "ws1",
                "status": "QUEUED",
                "task": task_id,
                "limit": 5
            } } }
    }));
    let listed = extract_tool_text(&jobs_list);
    assert!(
        listed
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_list must succeed: {listed}"
    );
    let jobs = listed
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!jobs.is_empty(), "expected at least one queued job");
    let job_id = jobs
        .first()
        .and_then(|v| v.get("job_id").and_then(|vv| vv.as_str()))
        .expect("job_id")
        .to_string();
    assert!(job_id.starts_with("JOB-"), "expected JOB-* id");

    let job_open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": "ws1",
                "job": job_id,
                "include_prompt": false,
                "include_events": false,
                "include_meta": true,
                "max_events": 0,
                "max_chars": 4000
            } } }
    }));
    let opened = extract_tool_text(&job_open);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_open must succeed: {opened}"
    );
    assert_eq!(
        opened
            .get("result")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("skill_profile"))
            .and_then(|v| v.as_str())
            .unwrap_or("-"),
        "strict",
        "macro_delegate must seed a deterministic skill_profile into job meta"
    );

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": {
                "workspace": "ws1",
                "max_chars": 3000
            } } }
    }));
    let snap = extract_tool_text(&snapshot);
    assert!(
        snap.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_snapshot must succeed: {snap}"
    );
    let hud_job_id = snap
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str());
    assert_eq!(
        hud_job_id,
        Some(job_id.as_str()),
        "expected active job HUD in snapshot capsule.where.job, got: {snap}"
    );

    let open_job = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws1", "id": job_id, "max_chars": 4000 } }
    }));
    let opened = extract_tool_text(&open_job);
    assert!(
        opened
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(JOB-*) must succeed: {opened}"
    );
    assert_eq!(
        opened
            .get("result")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("-"),
        "job",
        "open(JOB-*) must return kind=job"
    );
}

#[test]
fn completed_jobs_are_hidden_from_snapshot_hud() {
    let mut server = Server::start_initialized("completed_jobs_are_hidden_from_snapshot_hud");

    let delegate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.delegate", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Delegate Jobs",
                "task_title": "Core: close delegated job",
                "description": "Goal: ensure DONE jobs do not spam the HUD.",
                "resume_max_chars": 4000
            } } }
    }));
    let out = extract_tool_text_str(&delegate);
    let task_id = task_id_from_focus_line(&out).expect("expected focus TASK-* line");

    let jobs_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.list", "args": {
                "workspace": "ws1",
                "status": "QUEUED",
                "task": task_id,
                "limit": 5
            } } }
    }));
    let listed = extract_tool_text(&jobs_list);
    let jobs = listed
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let job_id = jobs
        .first()
        .and_then(|v| v.get("job_id").and_then(|vv| vv.as_str()))
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let _done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "status": "DONE", "summary": "ok", "refs": [] } } }
    }));

    let snapshot = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "workspace": "ws1", "max_chars": 3000 } } }
    }));
    let snap = extract_tool_text(&snapshot);
    let hud_job = snap
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("job"));
    assert!(
        hud_job.is_none(),
        "DONE jobs should not be shown in the HUD (capsule.where.job), got: {snap}"
    );
}

#[test]
fn tasks_jobs_complete_salvages_refs_from_summary_text() {
    let mut server =
        Server::start_initialized("tasks_jobs_complete_salvages_refs_from_summary_text");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Job: salvage proof refs",
                "prompt": "Do a thing and report proof.",
                "kind": "codex_cli",
                "priority": "LOW"
            } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);
    let summary = "CMD: cargo test -q\nLINK: https://ci.example.invalid/run/123\n";

    let _done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": {
                "workspace": "ws1",
                "job": job_id,
                "runner_id": "r1",
                "claim_revision": claim_revision,
                "status": "DONE",
                "summary": summary,
                "refs": []
            } } }
    }));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": "ws1",
                "job": job_id,
                "max_events": 20,
                "include_prompt": false,
                "include_meta": false
            } } }
    }));
    let opened_out = extract_tool_text(&opened);
    let events = opened_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let completed = events
        .iter()
        .find(|event| event.get("kind").and_then(|v| v.as_str()) == Some("completed"))
        .expect("expected a completed event");
    let refs = completed
        .get("refs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let refs_str = refs.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>();

    assert!(
        refs_str.contains(&"CMD: cargo test -q"),
        "expected CMD receipt ref to be salvaged from summary, got refs={refs_str:?}"
    );
    assert!(
        refs_str.contains(&"LINK: https://ci.example.invalid/run/123"),
        "expected LINK receipt ref to be salvaged from summary, got refs={refs_str:?}"
    );
    assert!(
        refs_str.contains(&job_id.as_str()),
        "expected job_id to be included in refs for navigability, got refs={refs_str:?}"
    );
}

#[test]
fn macro_fanout_jobs_creates_per_anchor_jobs() {
    let mut server = Server::start_initialized("macro_fanout_jobs_creates_per_anchor_jobs");

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Fanout",
                "task_title": "Investigate large change",
                "description": "Goal: split into per-anchor jobs.",
                "resume_max_chars": 4000
            } } }
    }));
    let out = extract_tool_text_str(&started);
    let task_id = task_id_from_focus_line(&out).expect("expected focus TASK-* line");

    let fanout = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.fanout.jobs", "args": {
                "workspace": "ws1",
                "task": task_id,
                "anchors": ["a:core", "a:storage", "a:mcp"],
                "prompt": "Investigate the change for this anchor and report stable refs.",
                "job_kind": "codex_cli",
                "job_priority": "MEDIUM"
            } } }
    }));
    let fanout_out = extract_tool_text(&fanout);
    assert!(
        fanout_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_macro_fanout_jobs must succeed: {fanout_out}"
    );
    let jobs = fanout_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(jobs.len(), 3, "expected 3 jobs, got: {jobs:?}");
    for j in &jobs {
        let job_id = j.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
        let anchor = j.get("anchor").and_then(|v| v.as_str()).unwrap_or("-");
        let created_ref = j.get("created_ref").and_then(|v| v.as_str()).unwrap_or("-");
        assert!(
            job_id.starts_with("JOB-"),
            "expected job_id=JOB-*, got: {job_id}"
        );
        assert!(
            anchor.starts_with("a:"),
            "expected anchor=a:*, got: {anchor}"
        );
        assert!(
            created_ref.starts_with("JOB-") && created_ref.contains('@'),
            "expected created_ref=JOB-*@seq, got: {created_ref}"
        );

        // Fan-out jobs should carry deterministic skill meta so any runner can behave consistently.
        let job_open = server.request(json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "tools/call",
            "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                    "workspace": "ws1",
                    "job": job_id,
                    "include_prompt": false,
                    "include_events": false,
                    "include_meta": true,
                    "max_events": 0,
                    "max_chars": 4000
                } } }
        }));
        let opened = extract_tool_text(&job_open);
        assert!(
            opened
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            "tasks_jobs_open must succeed: {opened}"
        );
        let profile = opened
            .get("result")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("skill_profile"))
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        assert!(
            matches!(profile, "strict" | "deep"),
            "expected skill_profile in job meta, got: {profile}"
        );
    }
}

#[test]
fn macro_merge_report_publishes_a_pinned_report() {
    let mut server = Server::start_initialized("macro_merge_report_publishes_a_pinned_report");

    let started = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": {
                "workspace": "ws1",
                "plan_title": "Plan Merge",
                "task_title": "Merge delegated results",
                "description": "Goal: merge job results into one pinned report.",
                "resume_max_chars": 4000
            } } }
    }));
    let out = extract_tool_text_str(&started);
    let task_id = task_id_from_focus_line(&out).expect("expected focus TASK-* line");

    let fanout = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.fanout.jobs", "args": {
                "workspace": "ws1",
                "task": task_id,
                "anchors": ["a:core", "a:storage", "a:mcp"],
                "prompt": "Investigate and produce refs.",
                "job_kind": "codex_cli",
                "job_priority": "MEDIUM"
            } } }
    }));
    let fanout_out = extract_tool_text(&fanout);
    let jobs = fanout_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let job_ids = jobs
        .iter()
        .filter_map(|j| {
            j.get("job_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(job_ids.len(), 3, "expected 3 job ids");

    for (idx, job_id) in job_ids.iter().enumerate() {
        let claim_revision = claim_job(&mut server, "ws1", job_id, "r1", None, false);
        let _done = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + idx,
            "method": "tools/call",
            "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": {
                    "workspace": "ws1",
                    "job": job_id,
                    "runner_id": "r1",
                    "claim_revision": claim_revision,
                    "status": "DONE",
                    "summary": "ok",
                    "refs": [format!("CARD-{}", idx + 1)]
                } } }
        }));
    }

    let merged = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.merge.report", "args": {
                "workspace": "ws1",
                "task": task_id,
                "jobs": job_ids,
                "title": "MERGE REPORT — smoke",
                "pin": true
            } } }
    }));
    let merged_out = extract_tool_text(&merged);
    assert!(
        merged_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_macro_merge_report must succeed: {merged_out}"
    );
    let published_card_id = merged_out
        .get("result")
        .and_then(|v| v.get("published_card_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert!(
        published_card_id.starts_with("CARD-PUB-"),
        "expected published_card_id to be a published card, got: {published_card_id}"
    );
}

#[test]
fn jobs_create_accepts_priority_normal_and_open_include_meta_roundtrips() {
    let mut server = Server::start_initialized(
        "jobs_create_accepts_priority_normal_and_open_include_meta_roundtrips",
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Delegation job",
                "prompt": "Do a small no-op task.",
                "priority": "normal",
                "meta": { "foo": "bar", "n": 1 }
            } } }
    }));
    let out = extract_tool_text(&created);
    assert!(
        out.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_create must succeed: {out}"
    );
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let priority = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("priority"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(
        priority, "MEDIUM",
        "priority normal must normalize to MEDIUM"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": "ws1",
                "job": job_id,
                "include_prompt": false,
                "include_events": false,
                "include_meta": true,
                "max_events": 0
            } } }
    }));
    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_open must succeed: {opened_out}"
    );
    let meta = opened_out
        .get("result")
        .and_then(|v| v.get("meta"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        meta.get("foo").and_then(|v| v.as_str()).unwrap_or("-"),
        "bar",
        "meta must roundtrip"
    );
    assert_eq!(
        meta.get("n").and_then(|v| v.as_i64()).unwrap_or(-1),
        1,
        "meta must roundtrip"
    );
}

#[test]
fn jobs_report_kind_and_meta_update_and_open_surfaces_them() {
    let mut server =
        Server::start_initialized("jobs_report_kind_and_meta_update_and_open_surfaces_them");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Long job",
                "prompt": "Do something.",
                "meta": { "state": "created" }
            } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let report = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": {
                "workspace": "ws1",
                "job": job_id,
                "runner_id": "r1",
                "claim_revision": claim_revision,
                "kind": "heartbeat",
                "message": "alive",
                "percent": 1,
                "meta": { "state": "running", "slice": 3 }
            } } }
    }));
    let reported = extract_tool_text(&report);
    assert!(
        reported
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_report must succeed: {reported}"
    );
    let event_kind = reported
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(event_kind, "heartbeat", "event.kind must preserve kind");

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": "ws1",
                "job": job_id,
                "include_prompt": false,
                "include_events": true,
                "include_meta": true,
                "max_events": 5,
                "max_chars": 4000
            } } }
    }));
    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_open must succeed: {opened_out}"
    );
    let meta = opened_out
        .get("result")
        .and_then(|v| v.get("meta"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        meta.get("state").and_then(|v| v.as_str()).unwrap_or("-"),
        "running",
        "jobs.meta must update via report"
    );
    let events = opened_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!events.is_empty(), "expected at least one event");
    let first_kind = events
        .first()
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(first_kind, "heartbeat", "open must include event kind");
}

#[test]
fn jobs_message_posts_manager_event_for_queued_and_running_jobs() {
    let mut server =
        Server::start_initialized("jobs_message_posts_manager_event_for_queued_and_running_jobs");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Interactive job",
                "prompt": "Work, but ask questions when blocked."
            } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let msg1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.message", "args": { "workspace": "ws1", "job": job_id, "message": "manager: start with the cheapest falsifier test" } } }
    }));
    let msg1_out = extract_tool_text(&msg1);
    assert!(
        msg1_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_message must succeed for QUEUED jobs: {msg1_out}"
    );
    assert_eq!(
        msg1_out
            .get("result")
            .and_then(|v| v.get("event"))
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("-"),
        "manager",
        "event.kind must be manager"
    );

    let _claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let msg2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.message", "args": { "workspace": "ws1", "job": job_id, "message": "manager: confirm the result via a focused test, then report back" } } }
    }));
    let msg2_out = extract_tool_text(&msg2);
    assert!(
        msg2_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_message must succeed for RUNNING jobs: {msg2_out}"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": {
                "workspace": "ws1",
                "job": job_id,
                "include_prompt": false,
                "include_events": true,
                "max_events": 20,
                "max_chars": 6000
            } } }
    }));
    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_open must succeed: {opened_out}"
    );
    let events = opened_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        events
            .iter()
            .any(|e| { e.get("kind").and_then(|v| v.as_str()) == Some("manager") }),
        "expected at least one manager event in job history"
    );
}

#[test]
fn jobs_message_rejects_terminal_jobs() {
    let mut server = Server::start_initialized("jobs_message_rejects_terminal_jobs");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Done job", "prompt": "noop" } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);
    let _done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "status": "DONE" } } }
    }));

    let msg = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.message", "args": { "workspace": "ws1", "job": job_id, "message": "manager: late message" } } }
    }));
    let msg_out = extract_tool_text(&msg);
    assert!(
        !msg_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "expected tasks_jobs_message to fail for terminal jobs: {msg_out}"
    );
    assert_eq!(
        msg_out
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("-"),
        "CONFLICT",
        "expected CONFLICT error for terminal job message"
    );
}

#[test]
fn jobs_portal_fmt_lines_renders_list_open_and_message() {
    let mut server =
        Server::start_initialized("jobs_portal_fmt_lines_renders_list_open_and_message");

    let _j1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "J1", "prompt": "noop" } } }
    }));
    let _j2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "J2", "prompt": "noop" } } }
    }));

    let list_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.list", "args": { "workspace": "ws1", "limit": 1, "fmt": "lines" } } }
    }));
    let list_text = extract_tool_text_str(&list_lines);
    assert!(
        list_text.contains("jobs count="),
        "expected jobs list header, got:\n{list_text}"
    );
    assert!(
        list_text.contains("JOB-"),
        "expected at least one job id line, got:\n{list_text}"
    );
    assert!(
        list_text.contains("MORE:"),
        "expected MORE hint when limit=1, got:\n{list_text}"
    );

    // Pick a job id from the list and post a message.
    let job_id = list_text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .find(|id| id.starts_with("JOB-"))
        .expect("JOB-* line")
        .to_string();

    let msg_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.message", "args": { "workspace": "ws1", "job": job_id, "message": "manager: hello", "fmt": "lines" } } }
    }));
    let msg_text = extract_tool_text_str(&msg_lines);
    assert!(
        msg_text.contains("message posted"),
        "expected message confirmation, got:\n{msg_text}"
    );

    let open_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": { "workspace": "ws1", "job": job_id, "max_events": 1, "fmt": "lines" } } }
    }));
    let open_text = extract_tool_text_str(&open_lines);
    assert!(
        open_text.contains("events:"),
        "expected events section, got:\n{open_text}"
    );
    assert!(
        open_text.contains("MORE:"),
        "expected MORE before_seq hint, got:\n{open_text}"
    );
    assert!(
        open_text.contains(&format!("{job_id}@")),
        "expected events to include copy/paste ref JOB-...@seq, got:\n{open_text}"
    );
}

#[test]
fn tasks_snapshot_marks_job_question_in_state_line() {
    let mut server = Server::start_initialized("tasks_snapshot_marks_job_question_in_state_line");

    let delegate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.delegate", "args": {
                "workspace": "ws1",
                "task_title": "DX: job question marker",
                "description": "Ensure snapshot shows when a job needs manager input.",
                "resume_max_chars": 4000
            } } }
    }));
    let task_out = extract_tool_text_str(&delegate);
    let task_id = task_id_from_focus_line(&task_out).expect("expected focus TASK-* line");

    let jobs_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.list", "args": { "workspace": "ws1", "task": task_id, "limit": 5 } } }
    }));
    let list_out = extract_tool_text(&jobs_list);
    let job_id = list_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let _q = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "question", "message": "Need a decision", "refs": [ job_id ] } } }
    }));

    let snap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "workspace": "ws1", "task": task_id, "fmt": "lines" } } }
    }));
    let snap_text = extract_tool_text_str(&snap);
    let first = snap_text.lines().next().unwrap_or("");
    assert!(
        first.contains("(RUNNING?)") || first.contains("(?") || first.contains("question"),
        "expected job question marker in state line, got:\n{snap_text}"
    );
}

#[test]
fn jobs_claim_can_reclaim_stale_running_jobs_with_allow_stale() {
    let mut server =
        Server::start_initialized("jobs_claim_can_reclaim_stale_running_jobs_with_allow_stale");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Stale job",
                "prompt": "Do something slow."
            } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let _claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", Some(1000), false);

    // Ensure it becomes stale (no heartbeats).
    sleep(Duration::from_millis(1200));

    let reclaimed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.claim", "args": {
                "workspace": "ws1",
                "job": job_id,
                "runner_id": "r2",
                "allow_stale": true,
                "lease_ttl_ms": 1000
            } } }
    }));
    let reclaimed_out = extract_tool_text(&reclaimed);
    assert!(
        reclaimed_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "reclaim must succeed: {reclaimed_out}"
    );
    let event_kind = reclaimed_out
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(event_kind, "reclaimed", "expected reclaimed event kind");

    let meta = reclaimed_out
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("meta"))
        .and_then(|v| v.as_object())
        .expect("reclaimed event should include meta");
    assert_eq!(
        meta.get("reason").and_then(|v| v.as_str()).unwrap_or("-"),
        "ttl_expired",
        "reclaim reason must be explicit"
    );
    assert_eq!(
        meta.get("previous_runner_id")
            .and_then(|v| v.as_str())
            .unwrap_or("-"),
        "r1",
        "previous_runner_id must identify the old runner"
    );
}

#[test]
fn jobs_report_heartbeat_is_coalesced_to_avoid_spam() {
    let mut server = Server::start_initialized("jobs_report_heartbeat_is_coalesced_to_avoid_spam");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": {
                "workspace": "ws1",
                "title": "Heartbeat coalesce",
                "prompt": "Do something slow."
            } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let _hb1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "heartbeat", "message": "hb1" } } }
    }));
    let _hb2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "heartbeat", "message": "hb2" } } }
    }));

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": { "workspace": "ws1", "job": job_id, "include_events": true, "max_events": 50, "max_chars": 8000 } } }
    }));
    let opened_out = extract_tool_text(&opened);
    assert!(
        opened_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "tasks_jobs_open must succeed: {opened_out}"
    );
    let events = opened_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let heartbeats = events
        .iter()
        .filter(|e| e.get("kind").and_then(|v| v.as_str()) == Some("heartbeat"))
        .collect::<Vec<_>>();
    assert_eq!(
        heartbeats.len(),
        1,
        "expected a single coalesced heartbeat event"
    );
    let msg = heartbeats[0]
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(
        msg, "hb2",
        "coalesced heartbeat should keep the latest message"
    );
}

#[test]
fn jobs_open_supports_before_seq_paging() {
    let mut server = Server::start_initialized("jobs_open_supports_before_seq_paging");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Paged job", "prompt": "Emit events." } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    for (idx, msg) in ["e1", "e2", "e3", "e4"].iter().enumerate() {
        let _ = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + idx as i64,
            "method": "tools/call",
            "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "checkpoint", "message": msg } } }
        }));
    }

    let page1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": { "workspace": "ws1", "job": job_id, "include_events": true, "max_events": 2, "max_chars": 8000 } } }
    }));
    let page1_out = extract_tool_text(&page1);
    assert!(
        page1_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "page1 open must succeed: {page1_out}"
    );
    let page1_events = page1_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(page1_events.len(), 2, "expected 2 events on first page");
    assert!(
        page1_out
            .get("result")
            .and_then(|v| v.get("has_more_events"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "expected has_more_events=true on first page"
    );
    let before_seq = page1_events
        .last()
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("seq");

    let page2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.open", "args": { "workspace": "ws1", "job": job_id, "include_events": true, "max_events": 2, "before_seq": before_seq, "max_chars": 8000 } } }
    }));
    let page2_out = extract_tool_text(&page2);
    assert!(
        page2_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "page2 open must succeed: {page2_out}"
    );
    let page2_events = page2_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!page2_events.is_empty(), "expected some events on page2");
    let newest_page2 = page2_events
        .first()
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(i64::MAX);
    assert!(
        newest_page2 < before_seq,
        "paging must return events older than before_seq"
    );
}

#[test]
fn jobs_radar_includes_attention_and_last_event() {
    let mut server = Server::start_initialized("jobs_radar_includes_attention_and_last_event");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Radar job", "prompt": "noop" } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);
    let question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "question", "message": "need manager answer" } } }
    }));
    let question_out = extract_tool_text(&question);
    let question_seq = question_out
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("event.seq");
    let expected_ref = format!("{job_id}@{question_seq}");

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10 } } }
    }));
    let radar_out = extract_tool_text(&radar);
    let jobs = radar_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!jobs.is_empty(), "expected at least one radar job row");

    let row = jobs
        .iter()
        .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job_id.as_str()))
        .expect("radar must include created job");

    let last_kind = row
        .get("last")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(last_kind, "question", "expected last.kind=question");

    let needs_manager = row
        .get("attention")
        .and_then(|v| v.get("needs_manager"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(needs_manager, "expected attention.needs_manager=true");

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10, "fmt": "lines" } } }
    }));
    let radar_text = extract_tool_text_str(&radar_lines);
    let job_line = radar_text
        .lines()
        .find(|line| line.trim_start().starts_with(expected_ref.as_str()))
        .expect("expected the job row to be ref-first and start with JOB-...@seq");
    let mut parts = job_line.split_whitespace();
    assert_eq!(
        parts.next(),
        Some(expected_ref.as_str()),
        "expected jobs_radar fmt=lines to be ref-first for stable navigation"
    );
    assert_eq!(
        parts.next(),
        Some("?"),
        "expected needs_manager marker to be present as a separate token"
    );
    assert!(
        job_line.contains(&format!("open id={expected_ref}")),
        "expected copy/paste open hint to include the last.ref"
    );
    assert!(
        job_line.contains(&format!("reply reply_job={job_id}")),
        "expected reply hint when needs_manager=true"
    );
}

#[test]
fn jobs_radar_defaults_to_lines_in_daily_toolset() {
    let mut server = Server::start_initialized_with_args(
        "jobs_radar_defaults_to_lines_in_daily_toolset",
        &["--toolset", "daily"],
    );

    let _j1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "J1", "prompt": "noop" } } }
    }));

    // In the daily toolset, jobs_radar is an inbox view and should default to BM-L1 lines.
    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10 } } }
    }));
    let radar_lines_out = extract_tool_text(&radar_lines);
    let radar_lines_text = radar_lines_out
        .as_str()
        .expect("daily jobs_radar should default to fmt=lines text");
    assert!(
        radar_lines_text
            .lines()
            .next()
            .unwrap_or("")
            .contains("jobs_radar"),
        "expected jobs_radar header line, got:\n{radar_lines_text}"
    );
    assert!(
        radar_lines_text.contains("JOB-"),
        "expected at least one job line, got:\n{radar_lines_text}"
    );
}

#[test]
fn jobs_tail_increments_after_seq_and_is_ascending() {
    let mut server = Server::start_initialized("jobs_tail_increments_after_seq_and_is_ascending");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Tail job", "prompt": "noop" } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);
    let _p1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "progress", "message": "p1" } } }
    }));
    let _p2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "progress", "message": "p2" } } }
    }));

    let page1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.tail", "args": { "workspace": "ws1", "job": job_id, "after_seq": 0, "limit": 2 } } }
    }));
    let page1_out = extract_tool_text(&page1);
    let next_after_seq = page1_out
        .get("result")
        .and_then(|v| v.get("next_after_seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(next_after_seq > 0, "expected next_after_seq to advance");

    let events1 = page1_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(events1.len(), 2, "expected limit=2 events on first page");
    let seq1 = events1
        .first()
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let seq2 = events1
        .get(1)
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(seq1 > 0 && seq2 > seq1, "expected ascending seqs");

    let page2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.tail", "args": { "workspace": "ws1", "job": job_id, "after_seq": next_after_seq, "limit": 50 } } }
    }));
    let page2_out = extract_tool_text(&page2);
    let events2 = page2_out
        .get("result")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for ev in &events2 {
        let seq = ev.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
        assert!(seq > next_after_seq, "tail must return seq > after_seq");
    }
}

#[test]
fn jobs_radar_and_tail_fmt_lines_render_smoke() {
    let mut server = Server::start_initialized("jobs_radar_and_tail_fmt_lines_render_smoke");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Lines job", "prompt": "noop" } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let radar_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "fmt": "lines", "limit": 5 } } }
    }));
    let radar_text = extract_tool_text_str(&radar_lines);
    assert!(
        radar_text.contains("jobs_radar count="),
        "expected radar header, got:\n{radar_text}"
    );
    assert!(
        radar_text.contains("JOB-"),
        "expected at least one JOB-* line, got:\n{radar_text}"
    );

    let tail_lines = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.tail", "args": { "workspace": "ws1", "job": job_id, "after_seq": 0, "fmt": "lines" } } }
    }));
    let tail_text = extract_tool_text_str(&tail_lines);
    assert!(
        tail_text.contains(" tail after_seq="),
        "expected tail header, got:\n{tail_text}"
    );
    assert!(
        tail_text.contains("events:"),
        "expected events section, got:\n{tail_text}"
    );
    assert!(
        tail_text.contains(&format!("{job_id}@")),
        "expected tail events to include copy/paste ref JOB-...@seq, got:\n{tail_text}"
    );
}

#[test]
fn jobs_requeue_moves_terminal_job_back_to_queued() {
    let mut server = Server::start_initialized("jobs_requeue_moves_terminal_job_back_to_queued");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Retry job", "prompt": "Fail once." } } }
    }));
    let out = extract_tool_text(&created);
    let job_id = out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);
    let _done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.complete", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "status": "FAILED", "summary": "failed" } } }
    }));

    let requeued = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.requeue", "args": { "workspace": "ws1", "job": job_id, "reason": "try again" } } }
    }));
    let requeued_out = extract_tool_text(&requeued);
    assert!(
        requeued_out
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "requeue must succeed: {requeued_out}"
    );
    let status = requeued_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(status, "QUEUED", "expected job to be QUEUED after requeue");
    let event_kind = requeued_out
        .get("result")
        .and_then(|v| v.get("event"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    assert_eq!(event_kind, "requeued", "expected requeued event kind");
}

#[test]
fn jobs_radar_needs_manager_is_sticky_until_manager_message() {
    let mut server =
        Server::start_initialized("jobs_radar_needs_manager_is_sticky_until_manager_message");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.create", "args": { "workspace": "ws1", "title": "Sticky question", "prompt": "noop" } } }
    }));
    let created_out = extract_tool_text(&created);
    let job_id = created_out
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim_revision = claim_job(&mut server, "ws1", &job_id, "r1", None, false);

    let _question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "question", "message": "Need a decision" } } }
    }));

    // Agent keeps working after asking — the question should remain visible to the manager.
    let _progress = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.report", "args": { "workspace": "ws1", "job": job_id, "runner_id": "r1", "claim_revision": claim_revision, "kind": "progress", "message": "Continuing while waiting" } } }
    }));

    let radar1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10 } } }
    }));
    let radar1_out = extract_tool_text(&radar1);
    let needs_manager_1 = radar1_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job_id.as_str()))
        })
        .and_then(|j| j.get("attention"))
        .and_then(|v| v.get("needs_manager"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        needs_manager_1,
        "expected needs_manager=true after question even with later progress"
    );

    let _answer = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.message", "args": { "workspace": "ws1", "job": job_id, "message": "Proceed with option A" } } }
    }));

    let radar2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "jobs", "arguments": { "op": "call", "cmd": "jobs.radar", "args": { "workspace": "ws1", "limit": 10 } } }
    }));
    let radar2_out = extract_tool_text(&radar2);
    let needs_manager_2 = radar2_out
        .get("result")
        .and_then(|v| v.get("jobs"))
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job_id.as_str()))
        })
        .and_then(|j| j.get("attention"))
        .and_then(|v| v.get("needs_manager"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        !needs_manager_2,
        "expected needs_manager=false after manager message"
    );
}
