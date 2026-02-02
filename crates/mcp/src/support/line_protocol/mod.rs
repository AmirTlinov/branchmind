#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;
use std::collections::HashMap;

mod util;
use util::*;

mod actions;
mod branchmind;
mod generic;

const FMT_JSON: &str = "json";
const FMT_LINES: &str = "lines";

// BM line protocol tags (speaking, tag-light).
const TAG_ERROR: &str = "ERROR";
const TAG_WARNING: &str = "WARNING";
const TAG_MORE: &str = "MORE";
const TAG_REFERENCE: &str = "REFERENCE";

pub(crate) fn is_lines_fmt(fmt: Option<&str>) -> bool {
    matches!(fmt, Some(FMT_LINES))
}

pub(crate) fn apply_portal_line_format(
    tool: &str,
    args: &Value,
    response: &mut Value,
    toolset: Toolset,
    omit_workspace: bool,
) {
    let fmt = args.get("fmt").and_then(|v| v.as_str()).unwrap_or(FMT_JSON);
    if !matches!(fmt, FMT_LINES) {
        return;
    }

    // Errors should always render as an explicit ERROR: line, regardless of tool.
    if response.get("error").and_then(|v| v.as_object()).is_some() {
        let rendered = generic::render_generic_lines(tool, args, response, toolset);
        if let Some(obj) = response.as_object_mut() {
            obj.insert("result".to_string(), Value::String(rendered));
            obj.insert("line_protocol".to_string(), Value::Bool(true));
            if obj.contains_key("warnings") {
                obj.insert("warnings".to_string(), Value::Array(Vec::new()));
            }
            if obj.contains_key("suggestions") {
                obj.insert("suggestions".to_string(), Value::Array(Vec::new()));
            }
        }
        return;
    }

    let rendered = match tool {
        "status" => branchmind::render_branchmind_status_lines(args, response, toolset),
        "workspace_use" => branchmind::render_branchmind_workspace_use_lines(args, response),
        "workspace_reset" => branchmind::render_branchmind_workspace_reset_lines(response),
        "macro_branch_note" => {
            branchmind::render_branchmind_macro_branch_note_lines(args, response, toolset)
        }
        "anchors_list" => branchmind::render_branchmind_anchors_list_lines(args, response, toolset),
        "anchor_snapshot" => {
            branchmind::render_branchmind_anchor_snapshot_lines(args, response, toolset)
        }
        "macro_anchor_note" => {
            branchmind::render_branchmind_macro_anchor_note_lines(args, response, toolset)
        }
        "anchors_export" => {
            branchmind::render_branchmind_anchors_export_lines(args, response, toolset)
        }
        "tasks_macro_start" => {
            render_tasks_macro_start_lines(args, response, toolset, omit_workspace)
        }
        "tasks_macro_delegate" => {
            render_tasks_macro_delegate_lines(args, response, toolset, omit_workspace)
        }
        "tasks_macro_close_step" => {
            render_tasks_macro_close_step_lines(args, response, toolset, omit_workspace)
        }
        "tasks_snapshot" => render_tasks_snapshot_lines(args, response, toolset, omit_workspace),
        "tasks_jobs_list" => render_tasks_jobs_list_lines(args, response, toolset, omit_workspace),
        "tasks_jobs_radar" => {
            render_tasks_jobs_radar_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_open" => render_tasks_jobs_open_lines(args, response, toolset, omit_workspace),
        "tasks_jobs_tail" => render_tasks_jobs_tail_lines(args, response, toolset, omit_workspace),
        "tasks_jobs_message" => {
            render_tasks_jobs_message_lines(args, response, toolset, omit_workspace)
        }
        // Unknown portal tool: render generic lines rather than silently doing nothing.
        _ => generic::render_generic_lines(tool, args, response, toolset),
    };

    if let Some(obj) = response.as_object_mut() {
        obj.insert("result".to_string(), Value::String(rendered));
        obj.insert("line_protocol".to_string(), Value::Bool(true));
        // Line protocol is intentionally low-noise: warnings/suggestions are rendered as lines
        // rather than repeated JSON envelopes.
        if obj.contains_key("warnings") {
            obj.insert("warnings".to_string(), Value::Array(Vec::new()));
        }
        if obj.contains_key("suggestions") {
            obj.insert("suggestions".to_string(), Value::Array(Vec::new()));
        }
    }
}

fn portalize_legacy_tool_call(
    legacy_tool: &str,
    args_value: Option<&Value>,
    outer_workspace: Option<&str>,
    omit_workspace: bool,
) -> Option<String> {
    let legacy_tool = legacy_tool.trim();
    if legacy_tool.is_empty() {
        return None;
    }
    // Already a v1 portal tool (or a method name like tools/list) — render directly.
    if crate::tools_v1::is_v1_tool(legacy_tool) {
        let args_str = args_value.and_then(render_kv_args).unwrap_or_default();
        return Some(if args_str.is_empty() {
            legacy_tool.to_string()
        } else {
            format!("{legacy_tool} {args_str}")
        });
    }

    let registry = crate::ops::CommandRegistry::global();
    let spec = registry.find_by_legacy_tool(legacy_tool)?;

    let mut inner = args_value
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Portal UX: default checkpoint set when omitted (copy/paste-safe discipline).
    if legacy_tool == "tasks_macro_close_step" && !inner.contains_key("checkpoints") {
        inner.insert("checkpoints".to_string(), Value::String("gate".to_string()));
    }

    // Hygiene: strip duplicated workspace inside nested args when we already have an outer workspace.
    if let Some(ws) = outer_workspace.map(|s| s.trim()).filter(|s| !s.is_empty())
        && let Some(inner_ws) = inner.get("workspace").and_then(|v| v.as_str())
        && inner_ws.trim() == ws
    {
        inner.remove("workspace");
    }

    let mut env = serde_json::Map::new();
    if !omit_workspace && let Some(ws) = outer_workspace.map(|s| s.trim()).filter(|s| !s.is_empty())
    {
        env.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    env.insert("op".to_string(), Value::String("call".to_string()));
    env.insert("cmd".to_string(), Value::String(spec.cmd.clone()));
    env.insert("args".to_string(), Value::Object(inner));

    let env_str = render_kv_args(&Value::Object(env)).unwrap_or_default();
    let portal = spec.domain_tool.as_str();
    Some(if env_str.is_empty() {
        portal.to_string()
    } else {
        format!("{portal} {env_str}")
    })
}

fn render_tasks_macro_start_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_start",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}

fn render_tasks_macro_delegate_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_delegate",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}

fn render_tasks_macro_close_step_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_close_step",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}

fn render_tasks_snapshot_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    // tasks_snapshot returns the resume_super payload directly.
    // Navigation guarantee (BM-L1): a stable `ref=` handle is embedded in the state line.
    render_tasks_resume_lines(
        toolset,
        "tasks_snapshot",
        args,
        response,
        &["result"],
        omit_workspace,
    )
}

fn render_tasks_jobs_list_lines(
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

fn render_tasks_jobs_radar_lines(
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

fn render_tasks_jobs_open_lines(
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

fn render_tasks_jobs_tail_lines(
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

fn render_tasks_jobs_message_lines(
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

fn render_tasks_resume_lines(
    _toolset: Toolset,
    tool: &str,
    args: &Value,
    response: &Value,
    resume_path: &[&str],
    omit_workspace: bool,
) -> String {
    let resume = get_at(response, resume_path).unwrap_or(&Value::Null);

    // Budget fallback may reduce the envelope to capsule-only.
    // Keep portal state lines meaningful by falling back to capsule coordinates.
    let focus = opt_str(resume.get("focus").and_then(|v| v.as_str()).or_else(|| {
        resume
            .get("capsule")
            .and_then(|v| v.get("focus"))
            .and_then(|v| v.as_str())
    }));
    let title = opt_str(
        resume
            .get("target")
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                resume
                    .get("capsule")
                    .and_then(|v| v.get("target"))
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
            }),
    );
    let status = resume
        .get("target")
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let next = opt_str(
        resume
            .get("radar")
            .and_then(|v| v.get("next"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str()),
    );

    let action_tool = resume
        .get("capsule")
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("tool"))
        .and_then(|v| v.as_str());
    let action_purpose = opt_str(
        resume
            .get("capsule")
            .and_then(|v| v.get("action"))
            .and_then(|v| v.get("purpose"))
            .and_then(|v| v.as_str()),
    );
    let action_args = resume
        .get("capsule")
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("args").or_else(|| v.get("args_hint")));

    let outer_ws = args.get("workspace").and_then(|v| v.as_str());
    let action_cmd = action_tool
        .and_then(|tool| portalize_legacy_tool_call(tool, action_args, outer_ws, omit_workspace));

    let prep_tool = resume
        .get("capsule")
        .and_then(|v| v.get("prep_action"))
        .and_then(|v| v.get("tool"))
        .and_then(|v| v.as_str());
    let prep_available = resume
        .get("capsule")
        .and_then(|v| v.get("prep_action"))
        .and_then(|v| v.get("available"))
        .and_then(|v| v.as_bool());
    let prep_args = resume
        .get("capsule")
        .and_then(|v| v.get("prep_action"))
        .and_then(|v| v.get("args").or_else(|| v.get("args_hint")));
    let prep_cmd = prep_tool
        .and_then(|tool| portalize_legacy_tool_call(tool, prep_args, outer_ws, omit_workspace));

    let map_tool = resume
        .get("capsule")
        .and_then(|v| v.get("map_action"))
        .and_then(|v| v.get("tool"))
        .and_then(|v| v.as_str());
    let map_available = resume
        .get("capsule")
        .and_then(|v| v.get("map_action"))
        .and_then(|v| v.get("available"))
        .and_then(|v| v.as_bool());
    let map_args = resume
        .get("capsule")
        .and_then(|v| v.get("map_action"))
        .and_then(|v| v.get("args").or_else(|| v.get("args_hint")));
    let map_cmd = map_tool
        .and_then(|tool| portalize_legacy_tool_call(tool, map_args, outer_ws, omit_workspace));

    let first_open_path = opt_str(
        resume
            .get("steps")
            .and_then(|v| v.get("first_open"))
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
    );
    let first_open_step = resume
        .get("steps")
        .and_then(|v| v.get("first_open"))
        .filter(|v| v.is_object());
    let open_steps = resume
        .get("steps")
        .and_then(|v| v.get("open"))
        .and_then(|v| v.as_u64());

    let (notes_more, notes_cursor) = pagination_more(
        resume,
        &["memory", "notes", "pagination"],
        &["memory", "notes", "pagination", "next_cursor"],
    );
    let (trace_more, trace_cursor) = pagination_more(
        resume,
        &["memory", "trace", "pagination"],
        &["memory", "trace", "pagination", "next_cursor"],
    );
    let (cards_more, cards_cursor) = pagination_more(
        resume,
        &["memory", "cards_pagination"],
        &["memory", "cards_pagination", "next_cursor"],
    );

    let mut more_cmd = None;
    let mut more_cmd_inner_args: Option<serde_json::Map<String, Value>> = None;
    if notes_more || trace_more || cards_more {
        let mut more_args = serde_json::Map::new();
        if notes_more && let Some(cursor) = notes_cursor {
            more_args.insert(
                "notes_cursor".to_string(),
                Value::Number(serde_json::Number::from(cursor)),
            );
        }
        if trace_more && let Some(cursor) = trace_cursor {
            more_args.insert(
                "trace_cursor".to_string(),
                Value::Number(serde_json::Number::from(cursor)),
            );
        }
        if cards_more && let Some(cursor) = cards_cursor {
            more_args.insert(
                "cards_cursor".to_string(),
                Value::Number(serde_json::Number::from(cursor)),
            );
        }

        // Continuation should be copy/paste-ready without asking the agent to "decode" cursors.
        // Prefer the read-only snapshot entrypoint for paging through memory.
        let more_value = Value::Object(more_args.clone());
        more_cmd = portalize_legacy_tool_call(
            "tasks_snapshot",
            Some(&more_value),
            outer_ws,
            omit_workspace,
        );
        more_cmd_inner_args = Some(more_args);
    }

    let mut lines = Vec::new();
    let mut state = match (focus, title) {
        (Some(focus), Some(title)) => format!("focus {focus} — {title}"),
        (Some(focus), None) => format!("focus {focus}"),
        (None, Some(title)) => format!("target {title}"),
        (None, None) => "ok".to_string(),
    };

    let where_id = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("map"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let where_unknown = where_id.as_deref() == Some("unknown");
    let where_needs_anchor = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("map"))
        .and_then(|v| v.get("needs_anchor"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Some(where_id) = where_id.as_deref() {
        state.push_str(" | where=");
        state.push_str(where_id);
    }

    if let Some(pack_ref) = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("pack"))
        .and_then(|v| v.get("ref"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        state.push_str(" | pack=");
        state.push_str(pack_ref);
    }

    if let Some(job) = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("job"))
        .and_then(|v| v.as_object())
    {
        let job_id = job
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| s.starts_with("JOB-") && !s.is_empty());
        if let Some(job_id) = job_id {
            state.push_str(" | job=");
            state.push_str(job_id);
            let last_kind = job
                .get("last")
                .and_then(|v| v.as_object())
                .and_then(|last| last.get("kind"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .unwrap_or("");

            let attention_obj = job.get("attention").and_then(|v| v.as_object());
            let needs_manager = attention_obj
                .and_then(|a| a.get("needs_manager"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let needs_proof = attention_obj
                .and_then(|a| a.get("needs_proof"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let has_error = attention_obj
                .and_then(|a| a.get("has_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let attention = if has_error {
                "!"
            } else if needs_manager {
                "?"
            } else if needs_proof {
                "!"
            } else {
                match last_kind {
                    "question" => "?",
                    "error" => "!",
                    _ => "",
                }
            };
            if let Some(status) = job
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "-")
            {
                state.push('(');
                state.push_str(status);
                state.push_str(attention);
                state.push(')');
            }
            // Low-noise hint: show a short last meaningful job update (excluding heartbeats).
            if last_kind != "heartbeat"
                && let Some(last) = job.get("last").and_then(|v| v.as_object())
                && let Some(message) = last
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && *s != "-")
            {
                let msg = truncate_line(message, 60);
                if !msg.is_empty() {
                    state.push_str(": ");
                    state.push_str(&msg);
                }
            }
        }
    }

    // Delegation UX: show inbox + runner liveness summary in the state line (counts, not lists).
    // This must be explicit (derived from persisted leases), not heuristic.
    let mut inbox_running = 0u64;
    let mut inbox_queued = 0u64;
    if let Some(inbox) = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("inbox"))
        .and_then(|v| v.as_object())
    {
        inbox_running = inbox.get("running").and_then(|v| v.as_u64()).unwrap_or(0);
        inbox_queued = inbox.get("queued").and_then(|v| v.as_u64()).unwrap_or(0);
        if inbox_running.saturating_add(inbox_queued) > 0 {
            state.push_str(" | inbox running=");
            state.push_str(&inbox_running.to_string());
            state.push_str(" queued=");
            state.push_str(&inbox_queued.to_string());
        }
    }

    if let Some(runner_status) = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("runner_status"))
        .and_then(|v| v.as_object())
    {
        let status = runner_status
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("-");
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
        let show = inbox_running.saturating_add(inbox_queued) > 0 || live_count + idle_count > 0;
        if show {
            state.push_str(" | runner=");
            state.push_str(status);
            // If jobs are RUNNING but there is no active lease, make the situation explicit.
            // This is a consistency check between two persisted facts, not a heuristic.
            if status == "offline" && inbox_running > 0 {
                state.push('!');
            }
            if live_count + idle_count + offline_count == 0 {
                // No known runner leases exist. Keep the display unambiguous and compact.
                state.push_str(" runners=none");
            } else if live_count + idle_count > 0 {
                // Product UX: hide offline leases when at least one runner is live/idle. Offline
                // leases are usually historical noise in that case.
                state.push_str(&format!(" runners=live:{live_count} idle:{idle_count}"));
            } else {
                state.push_str(&format!(
                    " runners=live:{live_count} idle:{idle_count} offline:{offline_count}"
                ));
            }
        }
    }

    // Horizons (anti-overload): show counts, not lists (plan-focused only).
    let horizon_obj = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("horizon"))
        .or_else(|| resume.get("radar").and_then(|v| v.get("horizon")));
    if let Some(horizon) = horizon_obj.and_then(|v| v.as_object()) {
        let active = horizon.get("active").and_then(|v| v.as_u64()).unwrap_or(0);
        let backlog = horizon.get("backlog").and_then(|v| v.as_u64()).unwrap_or(0);
        let parked = horizon.get("parked").and_then(|v| v.as_u64()).unwrap_or(0);
        let stale = horizon.get("stale").and_then(|v| v.as_u64()).unwrap_or(0);
        let done = horizon.get("done").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = horizon
            .get("total")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                active
                    .saturating_add(backlog)
                    .saturating_add(parked)
                    .saturating_add(done)
            });

        let over_active_limit = horizon
            .get("over_active_limit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if total > 0 {
            state.push_str(" | horizon active=");
            state.push_str(&active.to_string());
            if over_active_limit {
                state.push('!');
            }
            state.push_str(" backlog=");
            state.push_str(&backlog.to_string());
            state.push_str(" parked=");
            state.push_str(&parked.to_string());
            state.push_str(" stale=");
            state.push_str(&stale.to_string());
            state.push_str(" done=");
            state.push_str(&done.to_string());
            state.push_str(" total=");
            state.push_str(&total.to_string());
        }
    }

    // UX: keep a stable “copy/paste jump handle” in the first line.
    //
    // Flagship navigation guarantee (BM-L1): always emit `ref=<id>` even when `focus` is already
    // a stable TASK/PLAN id. This makes navigation robust under truncation and cheap for parsers.
    if let Some(card_id) = select_primary_card_reference_id(resume) {
        state.push_str(" | ref=");
        state.push_str(&card_id);
    } else if let Some(id) = resume
        .get("target")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            resume
                .get("capsule")
                .and_then(|v| v.get("target"))
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-")
    {
        state.push_str(" | ref=");
        state.push_str(id);
    } else if let Some(focus_id) = focus
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-")
    {
        state.push_str(" | ref=");
        state.push_str(focus_id);
    }

    // Keep the informational payload extremely small: one state line + a best-effort "next" hint.
    // Deep structured detail remains available via explicit full-view tools (e.g., tasks_resume_super).
    let map_primary = action_tool == Some("tasks_macro_close_step")
        && (where_unknown || where_needs_anchor)
        && map_available == Some(true)
        && map_cmd.is_some();

    let mut next_hint = if action_tool == Some("tasks_macro_close_step") {
        if let Some(path) = first_open_path {
            let mut hint = format!("next gate {path}");

            if let Some(first_open) = first_open_step {
                let missing = missing_checkpoints(first_open);
                if !missing.is_empty() {
                    // UX: this is informational, not an argument list. Keep it non-copy/paste-shaped.
                    hint.push_str(" needs(");
                    hint.push_str(&missing.join(" "));
                    hint.push(')');
                }

                let missing_proof = missing_proof(first_open);
                if !missing_proof.is_empty() {
                    hint.push_str(" proof(");
                    hint.push_str(&missing_proof.join(" "));
                    hint.push(')');
                }

                let next_action = first_open
                    .get("next_action")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty());
                if let Some(next_action) = next_action {
                    hint.push_str(" do(");
                    hint.push_str(&truncate_line(next_action, 56));
                    hint.push(')');
                }
            }

            if let Some(purpose) = action_purpose {
                hint.push_str(" (");
                hint.push_str(&truncate_line(purpose, 48));
                hint.push(')');
            }
            Some(hint)
        } else if open_steps == Some(0) {
            if let Some(purpose) = action_purpose {
                Some(format!("next finish ({})", truncate_line(purpose, 48)))
            } else {
                Some("next finish".to_string())
            }
        } else if let Some(purpose) = action_purpose {
            Some(format!("next gate ({})", truncate_line(purpose, 48)))
        } else {
            Some("next gate".to_string())
        }
    } else {
        next.map(|s| {
            if let Some(purpose) = action_purpose {
                format!("next {s} ({})", truncate_line(purpose, 48))
            } else {
                format!("next {s}")
            }
        })
    };
    if map_primary {
        next_hint = Some("next map".to_string());
    }
    if let Some(next_hint) = next_hint {
        state.push_str(" | ");
        state.push_str(&next_hint);
    } else if status == Some("DONE") {
        // If there's no recommended next action, give a tiny reason: the task is already DONE.
        // This avoids noisy ALREADY_DONE warnings in portals while keeping the state self-explanatory.
        state.push_str(" | done");
    }

    // Flagship anti-noise: keep BM-L1 to (state + command) on the happy path.
    //
    // If we have a useful secondary move, encode it as a single `backup` hint inside the state
    // line (not as an extra command line).
    let backup_cmd = if map_primary {
        action_cmd.clone()
    } else if action_tool == Some("tasks_macro_close_step")
        && prep_available == Some(true)
        && prep_tool.is_some()
    {
        prep_cmd.clone()
    } else if more_cmd.is_some() && action_cmd.is_some() {
        more_cmd.clone()
    } else {
        None
    };
    if let Some(backup_cmd) = backup_cmd {
        state.push_str(" | backup ");
        state.push_str(&backup_cmd);
    }

    let trimmed = simplified_trimmed_fields(resume, 3);
    let has_budget_warning = response
        .get("warnings")
        .and_then(|v| v.as_array())
        .is_some_and(|warnings| {
            warnings.iter().any(|w| {
                w.get("code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|code| code.starts_with("BUDGET_"))
            })
        });
    let was_trimmed = resume
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || !trimmed.is_empty()
        || has_budget_warning;
    if !trimmed.is_empty() {
        state.push_str(" | trimmed(");
        state.push_str(&trimmed.join(" "));
        state.push(')');
    }
    lines.push(state);

    // When jobs are queued but the runner is offline, surface a hunt-free copy/paste runner start hint.
    if let Some(cmd) = resume
        .get("capsule")
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("runner_bootstrap"))
        .and_then(|v| v.get("cmd"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        lines.push(format!("CMD: {cmd}"));
    }

    let want_delta = args.get("delta").and_then(|v| v.as_bool()).unwrap_or(false);
    if want_delta && let Some(delta) = resume.get("delta").and_then(|v| v.as_object()) {
        let mut first_delta_open_id: Option<String> = None;
        let order = [
            ("DECISION", "decisions", "id"),
            ("EVIDENCE", "evidence", "id"),
            ("CARD", "cards", "id"),
            ("NOTE", "notes", "ref"),
        ];
        for (label, section, id_key) in order {
            let items = delta
                .get(section)
                .and_then(|v| v.get("items"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for item in items {
                let id = item.get(id_key).and_then(|v| v.as_str()).unwrap_or("-");
                if first_delta_open_id.is_none() {
                    let id = id.trim();
                    if !id.is_empty() && id != "-" {
                        first_delta_open_id = Some(id.to_string());
                    }
                }
                let summary = item.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                if summary.trim().is_empty() {
                    lines.push(format!("{TAG_REFERENCE}: {label} {id}"));
                } else {
                    lines.push(format!(
                        "{TAG_REFERENCE}: {label} {id} | {}",
                        truncate_line(summary, 140)
                    ));
                }
            }
        }

        // Delta mode is often used for “what changed?” navigation. Provide one ready-to-run jump
        // command for the first referenced item to avoid manual id copying.
        //
        // If refs=true is requested, we already provide a separate navigation bundle; avoid
        // emitting duplicate open commands here.
        let want_refs = args.get("refs").and_then(|v| v.as_bool()).unwrap_or(false);
        if !want_refs && let Some(id) = first_delta_open_id {
            let mut open_args = serde_json::Map::new();
            if !omit_workspace
                && let Some(ws) = args.get("workspace").and_then(|v| v.as_str())
                && !ws.trim().is_empty()
            {
                open_args.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            open_args.insert("id".to_string(), Value::String(id));
            // Keep jump commands bounded to avoid a second “truncated” experience immediately
            // after following the portal’s suggestion.
            open_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(8000)),
            );
            let open_args = render_kv_args(&Value::Object(open_args)).unwrap_or_default();
            if open_args.is_empty() {
                lines.push("open".to_string());
            } else {
                lines.push(format!("open {open_args}"));
            }
        }
    }

    let has_action_cmd = action_cmd.is_some();

    // UX: navigation must be copy/paste-safe even under truncation.
    //
    // - `delta=true` already emits REFERENCE lines for changed items (and baseline seeding should
    //   stay low-noise), so we avoid adding extra refs there by default.
    // - For non-delta portals, if trimming happened (even via implicit/default budgets), emit a
    //   bounded set of openable references so an agent can jump to the exact card/doc entry.
    // - `refs=true` forces refs even when not truncated (explicit nav-mode).
    let want_refs = args.get("refs").and_then(|v| v.as_bool()).unwrap_or(false);
    // DX-DoD: when the portal already provides a single continuation command (no action),
    // keep the output strictly 2 lines. In that case, upgrade the continuation command into
    // nav-mode instead of adding extra REFERENCE lines.
    if !want_refs
        && !want_delta
        && was_trimmed
        && status == Some("DONE")
        && !has_action_cmd
        && more_cmd_inner_args.is_some()
    {
        // For DONE tasks, keep continuation strictly 2 lines (state + command). If budget trimming
        // happened, upgrade the continuation into nav-mode so the next call yields REFERENCE lines.
        if let Some(inner) = more_cmd_inner_args.as_mut()
            && !inner.contains_key("refs")
        {
            inner.insert("refs".to_string(), Value::Bool(true));
        }
        if let Some(inner) = more_cmd_inner_args.as_ref() {
            let inner_value = Value::Object(inner.clone());
            more_cmd = portalize_legacy_tool_call(
                "tasks_snapshot",
                Some(&inner_value),
                outer_ws,
                omit_workspace,
            );
        }
    } else if want_refs {
        append_budget_reference_lines(&mut lines, resume);
        let open_id = select_budget_reference_id(resume).or_else(|| {
            if tool != "tasks_snapshot" {
                return None;
            }
            response
                .get("snapshot_target_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| s.starts_with("TASK-") || s.starts_with("PLAN-"))
                .map(|s| s.to_string())
        });
        if let Some(id) = open_id {
            let mut open_args = serde_json::Map::new();
            if !omit_workspace
                && let Some(ws) = args.get("workspace").and_then(|v| v.as_str())
                && !ws.trim().is_empty()
            {
                open_args.insert("workspace".to_string(), Value::String(ws.to_string()));
            }
            open_args.insert("id".to_string(), Value::String(id));
            // Keep jump commands bounded to avoid a second “truncated” experience immediately
            // after following the portal’s suggestion.
            open_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(8000)),
            );
            let open_args = render_kv_args(&Value::Object(open_args)).unwrap_or_default();
            if open_args.is_empty() {
                lines.push("open".to_string());
            } else {
                lines.push(format!("open {open_args}"));
            }
        }
    }

    if map_primary {
        if let Some(cmd) = map_cmd {
            lines.push(cmd);
        }
    } else if let Some(cmd) = action_cmd {
        lines.push(cmd);
    } else if let Some(cmd) = more_cmd.clone() {
        // If there is no "next action" (e.g. focused task already DONE), but memory has more,
        // make continuation copy/paste-ready.
        lines.push(cmd);
    }

    if lines.is_empty() {
        lines.push("ok".to_string());
    }

    append_resume_warnings_as_warnings(&mut lines, args, response);
    lines.join("\n")
}

fn select_primary_card_reference_id(resume: &Value) -> Option<String> {
    fn has_tag(value: &Value, tag: &str) -> bool {
        let Some(tags) = value.get("tags").and_then(|v| v.as_array()) else {
            return false;
        };
        tags.iter().any(|t| {
            t.as_str()
                .map(|s| s.eq_ignore_ascii_case(tag))
                .unwrap_or(false)
        })
    }

    fn card_id(value: &Value) -> Option<&str> {
        value
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| s.starts_with("CARD-") && !s.is_empty() && *s != "-")
    }

    if let Some(cards) = resume
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        // Prefer pinned cockpit cards (frame + pinned) first: best “you are here” anchor.
        for card in cards {
            let Some(id) = card_id(card) else { continue };
            let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty.eq_ignore_ascii_case("frame") && has_tag(card, "pinned") {
                return Some(id.to_string());
            }
        }
        // Next: any pinned card.
        for card in cards {
            let Some(id) = card_id(card) else { continue };
            if has_tag(card, "pinned") {
                return Some(id.to_string());
            }
        }
        // Fallback: first card in the returned slice.
        if let Some(first) = cards.first().and_then(card_id) {
            return Some(first.to_string());
        }
    }

    // Budget fallback may reduce the envelope to capsule-only; capsule.refs retains precomputed
    // stable ids even when the cards slice is absent.
    if let Some(refs) = resume
        .get("capsule")
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
    {
        for r in refs {
            let Some(id) = r
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| s.starts_with("CARD-") && !s.is_empty() && *s != "-")
            else {
                continue;
            };
            return Some(id.to_string());
        }
    }

    None
}

fn append_budget_reference_lines(lines: &mut Vec<String>, resume: &Value) {
    // Deterministic, low-noise bounds.
    const MAX_CARD_REFS: usize = 2;
    const MAX_REFS_TOTAL: usize = 3;

    let before = lines.len();
    let mut added = 0usize;

    // Prefer card ids (stable) because they are the most useful "jump target".
    if let Some(cards) = resume
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        for card in cards.iter().take(MAX_CARD_REFS) {
            if added >= MAX_REFS_TOTAL {
                break;
            }
            let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let summary = card
                .get("title")
                .and_then(|v| v.as_str())
                .or_else(|| card.get("text").and_then(|v| v.as_str()))
                .unwrap_or("");
            if summary.trim().is_empty() {
                lines.push(format!("{TAG_REFERENCE}: CARD {id}"));
            } else {
                lines.push(format!(
                    "{TAG_REFERENCE}: CARD {id} | {}",
                    truncate_line(summary, 140)
                ));
            }
            added += 1;
        }
    }

    let notes_doc = resume
        .get("reasoning_ref")
        .and_then(|v| v.get("notes_doc"))
        .and_then(|v| v.as_str());
    if let (Some(notes_doc), Some(entries)) = (
        notes_doc,
        resume
            .get("memory")
            .and_then(|v| v.get("notes"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array()),
    ) {
        let max_seq = entries
            .iter()
            .filter_map(|e| e.get("seq").and_then(|v| v.as_i64()))
            .max();
        if let Some(seq) = max_seq
            && added < MAX_REFS_TOTAL
        {
            lines.push(format!("{TAG_REFERENCE}: NOTE {notes_doc}@{seq}"));
        }
    }

    // Trace is openable too, but we intentionally keep the default reference bundle small.

    // If the envelope was reduced to capsule-only, the memory/refs slices may be absent.
    // In that case, fall back to the capsule-provided refs (precomputed before budget fallback).
    if lines.len() == before
        && let Some(refs) = resume
            .get("capsule")
            .and_then(|v| v.get("refs"))
            .and_then(|v| v.as_array())
    {
        for r in refs.iter().take(MAX_REFS_TOTAL) {
            let Some(label) = r.get("label").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(id) = r.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            lines.push(format!("{TAG_REFERENCE}: {label} {id}"));
        }
    }

    // Absolute fallback: always keep at least one stable jump handle.
    // `open` supports TASK-* and PLAN-* and returns a navigation-oriented lens.
    if lines.len() == before
        && let Some(target_id) = resume
            .get("target")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                resume
                    .get("capsule")
                    .and_then(|v| v.get("target"))
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
            })
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "-")
    {
        let label = if target_id.starts_with("PLAN-") {
            "PLAN"
        } else {
            "TASK"
        };
        lines.push(format!("{TAG_REFERENCE}: {label} {target_id}"));
    }
}

fn select_budget_reference_id(resume: &Value) -> Option<String> {
    // Best overall jump handle: open the target itself. This yields a navigation-oriented lens
    // (capsule + reasoning refs + optional step focus) and stays stable even when the memory
    // slice was heavily degraded by budget constraints.
    if let Some(id) = resume
        .get("target")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            resume
                .get("capsule")
                .and_then(|v| v.get("target"))
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.trim())
        .filter(|s| s.starts_with("TASK-") || s.starts_with("PLAN-"))
    {
        return Some(id.to_string());
    }

    // Prefer stable card ids first: they are the best jump target for humans and agents.
    if let Some(id) = resume
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|card| card.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-")
    {
        return Some(id.to_string());
    }

    // Next best: the newest notes/traces entry (doc@seq), which is openable and gives the
    // agent an immediate “continue reading” surface even when cards are absent.
    let notes_doc = resume
        .get("reasoning_ref")
        .and_then(|v| v.get("notes_doc"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-");
    if let (Some(notes_doc), Some(entries)) = (
        notes_doc,
        resume
            .get("memory")
            .and_then(|v| v.get("notes"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array()),
    ) {
        let max_seq = entries
            .iter()
            .filter_map(|e| e.get("seq").and_then(|v| v.as_i64()))
            .max();
        if let Some(seq) = max_seq {
            return Some(format!("{notes_doc}@{seq}"));
        }
    }

    let trace_doc = resume
        .get("reasoning_ref")
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-");
    if let (Some(trace_doc), Some(entries)) = (
        trace_doc,
        resume
            .get("memory")
            .and_then(|v| v.get("trace"))
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array()),
    ) {
        let max_seq = entries
            .iter()
            .filter_map(|e| e.get("seq").and_then(|v| v.as_i64()))
            .max();
        if let Some(seq) = max_seq {
            return Some(format!("{trace_doc}@{seq}"));
        }
    }

    // Finally: budget fallback may degrade to capsule-only; use capsule.refs if present.
    if let Some(id) = resume
        .get("capsule")
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "-")
    {
        return Some(id.to_string());
    }

    None
}

fn missing_checkpoints(first_open: &Value) -> Vec<&'static str> {
    let mut missing = Vec::new();

    let security_confirmed = first_open
        .get("security_confirmed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let perf_confirmed = first_open
        .get("perf_confirmed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let docs_confirmed = first_open
        .get("docs_confirmed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let require_security = first_open
        .get("require_security")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let require_perf = first_open
        .get("require_perf")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let require_docs = first_open
        .get("require_docs")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if require_security && !security_confirmed {
        missing.push("security");
    }
    if require_perf && !perf_confirmed {
        missing.push("perf");
    }
    if require_docs && !docs_confirmed {
        missing.push("docs");
    }

    missing
}

fn missing_proof(first_open: &Value) -> Vec<&'static str> {
    let mut missing = Vec::new();

    let proof_tests_mode = first_open
        .get("proof_tests_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let proof_security_mode = first_open
        .get("proof_security_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let proof_perf_mode = first_open
        .get("proof_perf_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let proof_docs_mode = first_open
        .get("proof_docs_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let proof_tests_present = first_open
        .get("proof_tests_present")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_security_present = first_open
        .get("proof_security_present")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_perf_present = first_open
        .get("proof_perf_present")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_docs_present = first_open
        .get("proof_docs_present")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if proof_tests_mode == "require" && !proof_tests_present {
        missing.push("tests");
    }
    if proof_security_mode == "require" && !proof_security_present {
        missing.push("security");
    }
    if proof_perf_mode == "require" && !proof_perf_present {
        missing.push("perf");
    }
    if proof_docs_mode == "require" && !proof_docs_present {
        missing.push("docs");
    }

    missing
}
