#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;

const FMT_JSON: &str = "json";
const FMT_LINES: &str = "lines";

// BM line protocol tags (speaking, tag-light).
const TAG_ERROR: &str = "ERROR";
const TAG_WARNING: &str = "WARNING";
const TAG_MORE: &str = "MORE";

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
        let rendered = render_generic_lines(tool, args, response, toolset);
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
        "status" => render_branchmind_status_lines(args, response, toolset),
        "macro_branch_note" => render_branchmind_macro_branch_note_lines(args, response, toolset),
        "tasks_macro_start" => {
            render_tasks_macro_start_lines(args, response, toolset, omit_workspace)
        }
        "tasks_macro_close_step" => {
            render_tasks_macro_close_step_lines(args, response, toolset, omit_workspace)
        }
        "tasks_snapshot" => render_tasks_snapshot_lines(args, response, toolset, omit_workspace),
        // Unknown portal tool: render generic lines rather than silently doing nothing.
        _ => render_generic_lines(tool, args, response, toolset),
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

fn render_generic_lines(_tool: &str, _args: &Value, response: &Value, _toolset: Toolset) -> String {
    let mut lines = Vec::new();

    if let Some(err) = response.get("error").and_then(|v| v.as_object()) {
        let code = err.get("code").and_then(|v| v.as_str()).unwrap_or("ERROR");
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        let rec = err.get("recovery").and_then(|v| v.as_str());
        if let Some(rec) = rec {
            lines.push(format!("{TAG_ERROR}: {code} {msg} | fix: {rec}"));
        } else {
            lines.push(format!("{TAG_ERROR}: {code} {msg}"));
        }
        // Flagship invariant: keep recovery commands minimal.
        // If progressive disclosure is required, the server puts that first.
        append_suggestions_as_commands_limited(&mut lines, response, 2);
        return lines.join("\n");
    }

    let intent = response
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let success = response
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if success {
        lines.push(format!("ok intent={intent}"));
    } else {
        lines.push(format!("intent={intent}"));
    }
    // Generic fallback: let the agent discover the appropriate tool surface without
    // teaching "switch to json" as a habit.
    lines.push("tools/list toolset=full".to_string());
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

fn render_branchmind_status_lines(_args: &Value, response: &Value, _toolset: Toolset) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let checkout = result
        .get("checkout")
        .and_then(|v| v.as_str())
        .unwrap_or("-");

    let last_event = result
        .get("last_event")
        .and_then(|v| v.get("event_id"))
        .and_then(|v| v.as_str());
    let last_doc = result
        .get("last_doc_entry")
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str());

    let mut lines = Vec::new();
    let mut state = format!("ready checkout={checkout}");
    let mut last_parts = Vec::new();
    if let Some(event) = opt_str(last_event) {
        last_parts.push(format!("event={event}"));
    }
    if let Some(doc) = opt_str(last_doc) {
        last_parts.push(format!("doc={doc}"));
    }
    if !last_parts.is_empty() {
        state.push_str(" | last ");
        state.push_str(&last_parts.join(" "));
    }
    lines.push(state);

    // The portal output should remain tiny. Prefer a safe, read-only next step that can be run
    // immediately without extra parameters.
    lines.push("tasks_snapshot".to_string());
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

fn render_branchmind_macro_branch_note_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let name = result
        .get("branch")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let created = result
        .get("branch")
        .and_then(|v| v.get("created"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let base = result
        .get("branch")
        .and_then(|v| v.get("base_branch"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));
    let base_seq = result
        .get("branch")
        .and_then(|v| v.get("base_seq"))
        .and_then(|v| v.as_u64());
    let previous = result
        .get("checkout")
        .and_then(|v| v.get("previous"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));
    let current = result
        .get("checkout")
        .and_then(|v| v.get("current"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));
    let note_seq = result
        .get("note")
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let note_doc = result
        .get("note")
        .and_then(|v| v.get("doc"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));

    let mut lines = Vec::new();
    if created {
        if let (Some(base), Some(base_seq)) = (base, base_seq) {
            let mut line = format!("branch {name} from {base}@{base_seq}");
            let committed = if let Some(doc) = note_doc {
                format!("note committed {doc}@{note_seq}")
            } else {
                format!("note committed notes@{note_seq}")
            };
            line.push_str(" | ");
            line.push_str(&committed);
            lines.push(line);
        } else {
            let mut line = format!("branch {name}");
            let committed = if let Some(doc) = note_doc {
                format!("note committed {doc}@{note_seq}")
            } else {
                format!("note committed notes@{note_seq}")
            };
            line.push_str(" | ");
            line.push_str(&committed);
            lines.push(line);
        }
    } else if let (Some(prev), Some(curr)) = (previous, current)
        && prev != curr
    {
        let mut line = format!("checkout {prev} -> {curr}");
        let committed = if let Some(doc) = note_doc {
            format!("note committed {doc}@{note_seq}")
        } else {
            format!("note committed notes@{note_seq}")
        };
        line.push_str(" | ");
        line.push_str(&committed);
        lines.push(line);
    } else {
        // Default: no branch creation and no checkout change. Keep it to one line.
        if let Some(doc) = note_doc {
            lines.push(format!("note committed {doc}@{note_seq} on {name}"));
        } else {
            lines.push(format!("note committed notes@{note_seq} on {name}"));
        }
    }
    lines.push("status".to_string());
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
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
    render_tasks_resume_lines(
        toolset,
        "tasks_snapshot",
        args,
        response,
        &["result"],
        omit_workspace,
    )
}

fn render_tasks_resume_lines(
    _toolset: Toolset,
    _tool: &str,
    args: &Value,
    response: &Value,
    resume_path: &[&str],
    omit_workspace: bool,
) -> String {
    let resume = get_at(response, resume_path).unwrap_or(&Value::Null);

    let focus = opt_str(resume.get("focus").and_then(|v| v.as_str()));
    let title = opt_str(
        resume
            .get("target")
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str()),
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
    let action_available = resume
        .get("capsule")
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("available"))
        .and_then(|v| v.as_bool());
    let action_args = resume
        .get("capsule")
        .and_then(|v| v.get("action"))
        .and_then(|v| v.get("args").or_else(|| v.get("args_hint")));

    let action_cmd = action_tool.map(|tool| {
        let args_str = action_args.and_then(render_kv_args).unwrap_or_default();
        if args_str.is_empty() {
            tool.to_string()
        } else {
            format!("{tool} {args_str}")
        }
    });

    let escalation_toolset = resume
        .get("capsule")
        .and_then(|v| v.get("escalation"))
        .and_then(|v| v.get("toolset"))
        .and_then(|v| v.as_str());
    let escalation_required = resume
        .get("capsule")
        .and_then(|v| v.get("escalation"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

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
    if notes_more || trace_more || cards_more {
        let mut more_args = serde_json::Map::new();
        if !omit_workspace
            && let Some(ws) = args.get("workspace").and_then(|v| v.as_str())
            && !ws.trim().is_empty()
        {
            more_args.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
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
        // Prefer the read-only snapshot portal for paging through memory.
        let args_str = render_kv_args(&Value::Object(more_args)).unwrap_or_default();
        if args_str.is_empty() {
            more_cmd = Some("tasks_snapshot".to_string());
        } else {
            more_cmd = Some(format!("tasks_snapshot {args_str}"));
        }
    }

    let mut lines = Vec::new();
    let mut state = match (focus, title) {
        (Some(focus), Some(title)) => format!("focus {focus} — {title}"),
        (Some(focus), None) => format!("focus {focus}"),
        (None, Some(title)) => format!("target {title}"),
        (None, None) => "ok".to_string(),
    };

    // Keep the informational payload extremely small: one state line + a best-effort "next" hint.
    // Deep structured detail remains available via explicit full-view tools (e.g., tasks_resume_super).
    let next_hint = if action_tool == Some("tasks_macro_close_step") {
        if let Some(path) = first_open_path {
            let mut hint = format!("next gate {path}");

            if let Some(first_open) = first_open_step {
                let missing = missing_checkpoints(first_open);
                if !missing.is_empty() {
                    hint.push_str(" checkpoints(");
                    hint.push_str(&missing.join(" "));
                    hint.push(')');
                }

                let missing_proof = missing_proof(first_open);
                if !missing_proof.is_empty() {
                    hint.push_str(" proof(");
                    hint.push_str(&missing_proof.join(" "));
                    hint.push(')');
                }
            }

            Some(hint)
        } else if open_steps == Some(0) {
            Some("next finish".to_string())
        } else {
            Some("next gate".to_string())
        }
    } else {
        next.map(|s| format!("next {s}"))
    };
    if let Some(next_hint) = next_hint {
        state.push_str(" | ");
        state.push_str(&next_hint);
    } else if status == Some("DONE") {
        // If there's no recommended next action, give a tiny reason: the task is already DONE.
        // This avoids noisy ALREADY_DONE warnings in portals while keeping the state self-explanatory.
        state.push_str(" | done");
    }
    lines.push(state);

    let has_action_cmd = action_cmd.is_some();
    if let Some(cmd) = action_cmd {
        if action_available == Some(false)
            && escalation_required
            && let Some(toolset) = escalation_toolset
        {
            lines.push(format!("tools/list toolset={toolset}"));
        }
        lines.push(cmd);
    } else if let Some(cmd) = more_cmd.clone() {
        // If there is no "next action" (e.g. focused task already DONE), but memory has more,
        // make continuation copy/paste-ready.
        lines.push(cmd);
    }

    let mut more_parts = Vec::new();
    if notes_more {
        more_parts.push(format!("notes_cursor={}", notes_cursor.unwrap_or(0)));
    }
    if trace_more {
        more_parts.push(format!("trace_cursor={}", trace_cursor.unwrap_or(0)));
    }
    if cards_more {
        more_parts.push(format!("cards_cursor={}", cards_cursor.unwrap_or(0)));
    }
    if !more_parts.is_empty() && has_action_cmd {
        lines.push(format!("{TAG_MORE}: {}", more_parts.join(" ")));
    }

    if lines.is_empty() {
        lines.push("ok".to_string());
    }

    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

fn missing_checkpoints(first_open: &Value) -> Vec<&'static str> {
    let mut missing = Vec::new();

    let criteria_confirmed = first_open
        .get("criteria_confirmed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tests_confirmed = first_open
        .get("tests_confirmed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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

    if !criteria_confirmed {
        missing.push("criteria");
    }
    if !tests_confirmed {
        missing.push("tests");
    }
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

fn opt_str(value: Option<&str>) -> Option<&str> {
    value.filter(|s| {
        let trimmed = s.trim();
        !trimmed.is_empty() && trimmed != "-"
    })
}

fn append_warnings_as_warnings(lines: &mut Vec<String>, response: &Value) {
    let Some(warnings) = response.get("warnings").and_then(|v| v.as_array()) else {
        return;
    };
    for w in warnings {
        let code = w.get("code").and_then(|v| v.as_str()).unwrap_or("WARN");
        if code == "ALREADY_DONE" {
            // In portal UX, a DONE state line is enough — repeating "already done" as a warning
            // adds noise without improving recovery.
            continue;
        }
        let msg = w.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let rec = w.get("recovery").and_then(|v| v.as_str()).unwrap_or("");
        let mut out = format!("{TAG_WARNING}: {code}");
        if !msg.is_empty() {
            out.push(' ');
            out.push_str(msg);
        }
        if !rec.is_empty() {
            out.push_str(" | fix: ");
            out.push_str(rec);
        }
        lines.push(out);
    }
}

fn append_suggestions_as_commands_limited(lines: &mut Vec<String>, response: &Value, limit: usize) {
    if limit == 0 {
        return;
    }
    let Some(suggestions) = response.get("suggestions").and_then(|v| v.as_array()) else {
        return;
    };
    let mut added = 0usize;
    for s in suggestions {
        if added >= limit {
            break;
        }
        if let Some(cmd) = render_suggestion_command(s) {
            lines.push(cmd);
            added += 1;
        }
    }
}

fn render_suggestion_command(suggestion: &Value) -> Option<String> {
    let action = suggestion.get("action").and_then(|v| v.as_str())?;
    match action {
        "call_tool" => {
            let target = suggestion.get("target").and_then(|v| v.as_str())?;
            let params = suggestion.get("params").unwrap_or(&Value::Null);
            let args = render_kv_args(params).unwrap_or_default();
            if args.is_empty() {
                Some(target.to_string())
            } else {
                Some(format!("{target} {args}"))
            }
        }
        "call_method" => {
            let method = suggestion.get("method").and_then(|v| v.as_str())?;
            let params = suggestion.get("params").unwrap_or(&Value::Null);
            let args = render_kv_args(params).unwrap_or_default();
            if args.is_empty() {
                Some(method.to_string())
            } else {
                Some(format!("{method} {args}"))
            }
        }
        _ => None,
    }
}

fn get_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    Some(cur)
}

fn pagination_more(
    value: &Value,
    has_more_path: &[&str],
    cursor_path: &[&str],
) -> (bool, Option<u64>) {
    let has_more = get_at(value, has_more_path)
        .and_then(|v| v.get("has_more"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cursor = get_at(value, cursor_path).and_then(|v| v.as_u64());
    (has_more, cursor)
}

fn render_kv_args(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    if obj.is_empty() {
        return Some(String::new());
    }
    let mut keys = obj.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    let mut parts = Vec::new();
    for key in keys {
        let Some(v) = obj.get(&key) else {
            continue;
        };
        let rendered = match v {
            // For copy/paste-safe command lines, omit null keys entirely rather than emitting
            // `k=null`. Null in this protocol usually represents “unspecified/default”.
            Value::Null => continue,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => render_scalar_string(s),
            Value::Array(_) | Value::Object(_) => {
                serde_json::to_string(v).unwrap_or_else(|_| "?".to_string())
            }
        };
        parts.push(format!("{key}={rendered}"));
    }
    Some(parts.join(" "))
}

fn render_scalar_string(value: &str) -> String {
    // Keep common ids readable (no quotes), but quote when whitespace or punctuation would make
    // the command ambiguous. This is a UI protocol, not a shell, but we still want copy/paste-safe
    // tokens for agents.
    if is_safe_token(value) {
        return value.to_string();
    }
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}

fn is_safe_token(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed != value {
        return false;
    }
    // Allow a conservative "identifier-ish" alphabet for unquoted tokens.
    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '+'))
}
