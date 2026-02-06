#![forbid(unsafe_code)]

use super::super::*;

pub(super) fn render_tasks_jobs_list_lines(
    args: &Value,
    response: &Value,
    _toolset: Toolset,
    omit_workspace: bool,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let jobs = result
        .get("jobs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let count = result
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(jobs.len() as u64);
    let has_more = result
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let anchor = args
        .get("anchor")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let mut head_parts = Vec::new();
    head_parts.push(format!("jobs count={count}"));
    if let Some(status) = status {
        head_parts.push(format!("status={status}"));
    }
    if let Some(task) = task {
        head_parts.push(format!("task={task}"));
    }
    if let Some(anchor) = anchor {
        head_parts.push(format!("anchor={anchor}"));
    }
    if has_more || truncated {
        head_parts.push("has_more=true".to_string());
    }

    let mut lines = Vec::new();
    if omit_workspace {
        lines.push(head_parts.join(" "));
    } else {
        let ws = result
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        lines.push(format!("{ws} {}", head_parts.join(" ")));
    }

    for job in jobs {
        let id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
        let status = job.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
        let title = truncate_line(title, 80);

        let mut line = format!("{id} ({status}) {title}");
        if let Some(task) = job
            .get("task")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            line.push_str(" | task=");
            line.push_str(task);
        }
        if let Some(anchor) = job
            .get("anchor")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            line.push_str(" | anchor=");
            line.push_str(anchor);
        }
        lines.push(line);
    }

    if has_more || truncated {
        lines.push(format!(
            "{TAG_MORE}: Increase limit or filter by status/task/anchor."
        ));
    }
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_tasks_jobs_radar_lines(
    args: &Value,
    response: &Value,
    _toolset: Toolset,
    omit_workspace: bool,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let jobs = result
        .get("jobs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let count = result
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(jobs.len() as u64);
    let has_more = result
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let truncated = result
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let anchor = args
        .get("anchor")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let stale_after_s = args
        .get("stale_after_s")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0);

    let mut head_parts = Vec::new();
    head_parts.push(format!("jobs_radar count={count}"));
    let mut runner_status_str = None::<String>;
    if let Some(runner_status) = result.get("runner_status").and_then(|v| v.as_object()) {
        if let Some(s) = runner_status
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            runner_status_str = Some(s.to_string());
            head_parts.push(format!("runner={s}"));
        }
        let live_count = runner_status
            .get("live_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let idle_count = runner_status
            .get("idle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let offline_count = runner_status
            .get("offline_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if live_count
            .saturating_add(idle_count)
            .saturating_add(offline_count)
            == 0
        {
            head_parts.push("runners=none".to_string());
        } else if live_count.saturating_add(idle_count) > 0 {
            // Product UX: offline leases are usually historical noise when a live/idle runner exists.
            head_parts.push(format!("runners=live:{live_count} idle:{idle_count}"));
        } else {
            head_parts.push(format!(
                "runners=live:{live_count} idle:{idle_count} offline:{offline_count}"
            ));
        }
    }
    if let Some(status) = status {
        head_parts.push(format!("status={status}"));
    }
    if let Some(task) = task {
        head_parts.push(format!("task={task}"));
    }
    if let Some(anchor) = anchor {
        head_parts.push(format!("anchor={anchor}"));
    }
    if let Some(s) = stale_after_s {
        head_parts.push(format!("stale_after_s={s}"));
    }
    if has_more || truncated {
        head_parts.push("has_more=true".to_string());
    }

    let mut lines = Vec::new();
    if omit_workspace {
        lines.push(head_parts.join(" "));
    } else {
        let ws = result
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        lines.push(format!("{ws} {}", head_parts.join(" ")));
    }

    // When jobs are queued, surface a hunt-free copy/paste runner start hint.
    if let Some(cmd) = result
        .get("runner_bootstrap")
        .and_then(|v| v.get("cmd"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        lines.push(format!("CMD: {cmd}"));
    }

    // Multi-runner diagnostics (explicit leases, bounded).
    let mut runner_lines = Vec::<String>::new();
    let mut runner_status_by_id = HashMap::<String, String>::new();
    let mut runner_leases_complete = false;
    if let Some(leases) = result.get("runner_leases").and_then(|v| v.as_object()) {
        let has_more = leases
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        runner_leases_complete = !has_more;
        let runners = leases
            .get("runners")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let max_lines = args
            .get("runners_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(1, 15) as usize;

        // Build a bounded runner status map for job lines (avoid false “offline” when runners are
        // merely truncated from display).
        for runner in &runners {
            let rid_raw = runner
                .get("runner_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            let status = runner
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            if rid_raw != "-" && status != "-" {
                runner_status_by_id.insert(rid_raw.to_string(), status.to_string());
            }
        }

        for runner in runners.iter().take(max_lines) {
            let rid_raw = runner
                .get("runner_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            let rid_display = truncate_line(rid_raw, 60);
            let status = runner
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            let active_job = runner
                .get("active_job_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            let mut line = format!("runner {status} {rid_display}");
            if rid_raw != "-" {
                line.push_str(" | open id=runner:");
                line.push_str(rid_raw);
            }
            if let Some(job) = active_job {
                line.push_str(" job=");
                line.push_str(job);
                line.push_str(" | open id=");
                line.push_str(job);
            }
            runner_lines.push(line);
        }

        // If there are queued jobs and the runner is offline, show a cheap nudge.
        if runner_lines.is_empty()
            && runner_status_str.as_deref() == Some("offline")
            && result.get("runner_bootstrap").is_some()
        {
            runner_lines.push("runner offline (no active lease)".to_string());
        }

        if has_more || runners.len() > max_lines {
            runner_lines.push(format!("{TAG_MORE}: runners has_more=true"));
        }
    }
    lines.extend(runner_lines);

    // Recent offline runner IDs (explicit, no heuristics).
    if let Some(off) = result
        .get("runner_leases_offline")
        .and_then(|v| v.as_object())
    {
        let runners = off
            .get("runners")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let has_more = off
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let max_offline_lines = args
            .get("offline_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(3)
            .min(10) as usize;

        if max_offline_lines > 0 {
            for runner in runners.iter().take(max_offline_lines) {
                let rid_raw = runner
                    .get("runner_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("-");
                let rid_display = truncate_line(rid_raw, 60);
                let last_status = runner
                    .get("last_status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("-");
                let last_job = runner
                    .get("active_job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty());

                let mut line = format!("runner offline {rid_display}");
                if last_status != "-" {
                    line.push_str(" last=");
                    line.push_str(last_status);
                }
                if rid_raw != "-" {
                    line.push_str(" | open id=runner:");
                    line.push_str(rid_raw);
                }
                if let Some(job) = last_job {
                    line.push_str(" last_job=");
                    line.push_str(job);
                    line.push_str(" | open id=");
                    line.push_str(job);
                }
                lines.push(line);
            }

            if has_more || runners.len() > max_offline_lines {
                lines.push(format!("{TAG_MORE}: runners_offline has_more=true"));
            }
        }
    }

    // Runner conflict diagnostics (bounded). These are explicit consistency checks between
    // runner leases and job claim leases, not heuristics.
    if let Some(diag) = result.get("runner_diagnostics").and_then(|v| v.as_object()) {
        let issues = diag
            .get("issues")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let has_more = diag
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let max_issue_lines = 5usize;
        for issue in issues.iter().take(max_issue_lines) {
            let severity = issue
                .get("severity")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("warn");
            let marker = match severity {
                "stale" => "~",
                "error" => "!",
                "warn" => "!",
                "question" => "?",
                _ => "!",
            };

            let kind = issue
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("-");
            let msg = issue.get("message").and_then(|v| v.as_str()).unwrap_or("-");
            let msg = truncate_line(msg, 120);

            let runner_id = issue
                .get("runner_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let job_id = issue
                .get("job_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            let mut line = format!("{marker} diag {kind}");
            if let Some(rid) = runner_id {
                line.push_str(" runner=");
                line.push_str(&truncate_line(rid, 60));
            }
            if let Some(jid) = job_id {
                line.push_str(" job=");
                line.push_str(jid);
            }

            if let Some(rid) = runner_id {
                line.push_str(" | open id=runner:");
                line.push_str(rid);
                if let Some(jid) = job_id {
                    line.push_str(" | open id=");
                    line.push_str(jid);
                }
            } else if let Some(jid) = job_id {
                line.push_str(" | open id=");
                line.push_str(jid);
            }

            if !msg.is_empty() && msg != "-" {
                line.push_str(" | ");
                line.push_str(&msg);
            }

            lines.push(line);
        }

        if has_more || issues.len() > max_issue_lines {
            lines.push(format!("{TAG_MORE}: runner_diagnostics has_more=true"));
        }
    }

    for job in jobs {
        let id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
        let status = job.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
        let title = truncate_line(title, 80);
        let runner = job
            .get("runner")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let attention = job.get("attention").and_then(|v| v.as_object());
        let needs_manager = attention
            .and_then(|a| a.get("needs_manager"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let needs_proof = attention
            .and_then(|a| a.get("needs_proof"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let has_error = attention
            .and_then(|a| a.get("has_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let stale = attention
            .and_then(|a| a.get("stale"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let marker = if has_error {
            "!"
        } else if needs_manager {
            "?"
        } else if needs_proof {
            "!"
        } else if stale {
            "~"
        } else {
            ""
        };

        // Ref-first: make the stable navigation pointer the first token on the line.
        // The server guarantees a created event on job creation, so `last.ref` is expected to exist.
        let mut primary_ref = id.trim().to_string();
        let mut last_kind = None::<String>;
        let mut last_msg = None::<String>;
        if let Some(last) = job.get("last").and_then(|v| v.as_object()) {
            if let Some(last_ref) = last
                .get("ref")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "-")
            {
                primary_ref = last_ref.to_string();
            }

            last_kind = last
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "-")
                .map(|s| s.to_string());

            let msg = last.get("message").and_then(|v| v.as_str()).unwrap_or("-");
            let msg = truncate_line(msg, 80);
            if !msg.is_empty() && msg != "-" {
                last_msg = Some(msg);
            }
        }

        let mut line = String::new();
        line.push_str(&primary_ref);
        if !marker.is_empty() {
            line.push(' ');
            line.push_str(marker);
        }
        line.push(' ');
        line.push_str(id);
        line.push(' ');
        line.push('(');
        line.push_str(status);
        line.push(')');
        line.push(' ');
        line.push_str(&title);
        if status == "RUNNING"
            && let Some(runner) = runner
        {
            line.push_str(" runner=");
            let job_runner_state = job
                .get("runner_state")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            if let Some(state) = job_runner_state {
                // Preferred: explicit per-job runner_state computed from persisted leases.
                line.push_str(state);
                line.push(':');
            } else if let Some(runner_state) = runner_status_by_id.get(runner).map(|s| s.as_str()) {
                // Back-compat fallback for older servers: infer from the (possibly truncated)
                // runner leases list.
                line.push_str(runner_state);
                line.push(':');
            } else if runner_leases_complete {
                // Explicit, fact-based: if the active runner lease set is complete and we
                // don't see this runner, it's offline (no valid lease).
                line.push_str("offline:");
            } else {
                // Back-compat fallback: runner lease set is incomplete and we have no
                // per-job runner_state field, so we cannot classify it safely.
                line.push_str("unknown:");
            }
            line.push_str(&truncate_line(runner, 60));
        }

        // Always include the copy/paste next move.
        line.push_str(" | open id=");
        line.push_str(&primary_ref);
        if needs_manager {
            line.push_str(" | reply reply_job=");
            line.push_str(id);
            line.push_str(" reply_message=\"...\"");
        }

        // Optional last-event preview for quick scanning (never required for navigation).
        if let Some(msg) = last_msg {
            let kind = last_kind.as_deref().unwrap_or("-");
            if kind != "heartbeat" && kind != "-" {
                line.push_str(" | ");
                line.push_str(kind);
                line.push_str(": ");
            } else {
                line.push_str(" | ");
            }
            line.push_str(&msg);
        }

        lines.push(line);
    }

    if has_more || truncated {
        lines.push(format!(
            "{TAG_MORE}: Increase limit or filter by status/task/anchor."
        ));
    }

    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_tasks_jobs_open_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
    omit_workspace: bool,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let job = result.get("job").unwrap_or(&Value::Null);

    let job_id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
    let status = job.get("status").and_then(|v| v.as_str()).unwrap_or("-");
    let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
    let title = truncate_line(title, 80);

    let mut lines = Vec::new();
    if omit_workspace {
        lines.push(format!("job {job_id} ({status}) {title}"));
    } else {
        let ws = result
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        lines.push(format!("{ws} job {job_id} ({status}) {title}"));
    }

    let mut meta_parts = Vec::new();
    if let Some(task) = job
        .get("task")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        meta_parts.push(format!("task={task}"));
    }
    if let Some(anchor) = job
        .get("anchor")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        meta_parts.push(format!("anchor={anchor}"));
    }
    if let Some(runner) = job
        .get("runner")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        meta_parts.push(format!("runner={}", truncate_line(runner, 60)));
    }
    if !meta_parts.is_empty() {
        lines.push(meta_parts.join(" | "));
    }

    if let Some(summary) = job
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        lines.push(format!("summary: {}", truncate_line(summary, 140)));
    }

    if let Some(prompt) = result
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        lines.push(format!("prompt: {}", truncate_line(prompt, 160)));
    }

    let events = result
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !events.is_empty() {
        lines.push("events:".to_string());
    }
    for ev in &events {
        let seq = ev.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
        let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
        let msg = ev.get("message").and_then(|v| v.as_str()).unwrap_or("-");
        let msg = truncate_line(msg, 160);

        let mut line = format!("- {job_id}@{seq} {kind}: {msg}");
        let refs = ev
            .get("refs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(3)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !refs.is_empty() {
            line.push_str(" | refs: ");
            line.push_str(&refs.join(", "));
        }
        lines.push(line);
    }

    let has_more = result
        .get("has_more_events")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if has_more {
        let before_seq = events
            .last()
            .and_then(|v| v.get("seq"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if before_seq > 0 {
            lines.push(format!("{TAG_MORE}: before_seq={before_seq}"));
        } else {
            lines.push(format!(
                "{TAG_MORE}: Increase max_events or page with before_seq."
            ));
        }
    }

    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_tasks_jobs_tail_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
    omit_workspace: bool,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);

    let job_id = result.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
    let after_seq = result
        .get("after_seq")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let next_after_seq = result
        .get("next_after_seq")
        .and_then(|v| v.as_i64())
        .unwrap_or(after_seq);
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let has_more = result
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut head = format!(
        "job {job_id} tail after_seq={after_seq} next_after_seq={next_after_seq} count={count}"
    );
    if has_more {
        head.push_str(" has_more=true");
    }

    let mut lines = Vec::new();
    if omit_workspace {
        lines.push(head);
    } else {
        let ws = result
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        lines.push(format!("{ws} {head}"));
    }

    let events = result
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !events.is_empty() {
        lines.push("events:".to_string());
    }
    for ev in &events {
        let seq = ev.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
        let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
        let msg = ev.get("message").and_then(|v| v.as_str()).unwrap_or("-");
        let msg = truncate_line(msg, 160);

        let mut line = format!("- {job_id}@{seq} {kind}: {msg}");
        let refs = ev
            .get("refs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(3)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !refs.is_empty() {
            line.push_str(" | refs: ");
            line.push_str(&refs.join(", "));
        }
        lines.push(line);
    }

    if has_more {
        lines.push(format!("{TAG_MORE}: after_seq={next_after_seq}"));
    }

    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_tasks_jobs_message_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
    omit_workspace: bool,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let job = result.get("job").unwrap_or(&Value::Null);
    let ev = result.get("event").unwrap_or(&Value::Null);

    let job_id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
    let seq = ev.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
    let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
    let msg = ev.get("message").and_then(|v| v.as_str()).unwrap_or("-");

    let mut lines = Vec::new();
    if omit_workspace {
        lines.push(format!(
            "job {job_id} message posted | ref={job_id}@{seq} kind={kind}"
        ));
    } else {
        let ws = result
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        lines.push(format!(
            "{ws} job {job_id} message posted | ref={job_id}@{seq} kind={kind}"
        ));
    }
    lines.push(truncate_line(msg, 200));

    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}
