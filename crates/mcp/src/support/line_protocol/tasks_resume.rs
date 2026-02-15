#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;

use super::TAG_REFERENCE;
use super::portalize_handler_name_call;
use super::util::*;

pub(super) fn render_tasks_resume_lines(
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
        .and_then(|tool| portalize_handler_name_call(tool, action_args, outer_ws, omit_workspace));

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
        .and_then(|tool| portalize_handler_name_call(tool, prep_args, outer_ws, omit_workspace));

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
        .and_then(|tool| portalize_handler_name_call(tool, map_args, outer_ws, omit_workspace));

    let first_open_path = opt_str(
        resume
            .get("steps")
            .and_then(|v| v.get("first_open"))
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
    );
    let first_open_step_id = opt_str(
        resume
            .get("steps")
            .and_then(|v| v.get("first_open"))
            .and_then(|v| v.get("step_id"))
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
        more_cmd = portalize_handler_name_call(
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
            if let Some(step_id) = first_open_step_id {
                hint.push(' ');
                hint.push_str("step=");
                hint.push_str(step_id);
            }

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
            more_cmd = portalize_handler_name_call(
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
