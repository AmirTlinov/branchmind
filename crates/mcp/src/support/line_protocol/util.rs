#![forbid(unsafe_code)]

use serde_json::Value;

use super::TAG_WARNING;

pub(super) fn truncate_line(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if max_chars == 0 || text.is_empty() {
        return String::new();
    }
    let mut chars = text.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }
    if chars.next().is_some() && max_chars > 1 {
        out.pop();
        out.push('…');
    }
    out
}

pub(super) fn simplified_trimmed_fields(resume: &Value, max_items: usize) -> Vec<&'static str> {
    let Some(fields) = resume
        .get("degradation")
        .and_then(|v| v.get("truncated_fields"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    let mut out = Vec::<&'static str>::new();
    for f in fields {
        let Some(f) = f.as_str() else {
            continue;
        };
        let token = if f.contains("memory.notes") {
            "notes"
        } else if f.contains("memory.trace") {
            "trace"
        } else if f.contains("memory.cards") {
            "cards"
        } else if f.contains("signals") {
            "signals"
        } else if f.contains("timeline") {
            "timeline"
        } else if f.contains("graph_diff") {
            "graph_diff"
        } else if f.contains("engine") {
            "engine"
        } else if f.contains("capsule") {
            "capsule"
        } else {
            "other"
        };
        if !out.contains(&token) {
            out.push(token);
        }
        if out.len() >= max_items {
            break;
        }
    }
    out
}

pub(super) fn opt_str(value: Option<&str>) -> Option<&str> {
    value.filter(|s| {
        let trimmed = s.trim();
        !trimmed.is_empty() && trimmed != "-"
    })
}

pub(super) fn append_warnings_as_warnings(lines: &mut Vec<String>, response: &Value) {
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

pub(super) fn append_resume_warnings_as_warnings(
    lines: &mut Vec<String>,
    args: &Value,
    response: &Value,
) {
    let Some(warnings) = response.get("warnings").and_then(|v| v.as_array()) else {
        return;
    };

    // Portal defaults: keep tasks_snapshot output copy/paste-ready.
    // Budget warnings are useful when the caller opted into budgets; otherwise,
    // the state line includes a compact `trimmed(...)` marker for honesty.
    let budget_opt_in = args.get("max_chars").is_some()
        || args.get("context_budget").is_some()
        || args.get("resume_max_chars").is_some();

    for w in warnings {
        let code = w.get("code").and_then(|v| v.as_str()).unwrap_or("WARN");
        if code == "ALREADY_DONE" {
            continue;
        }
        if !budget_opt_in && code.starts_with("BUDGET_") {
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

pub(super) fn append_suggestions_as_commands_limited(
    lines: &mut Vec<String>,
    response: &Value,
    limit: usize,
) {
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

pub(super) fn render_suggestion_command(suggestion: &Value) -> Option<String> {
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

pub(super) fn get_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    Some(cur)
}

pub(super) fn pagination_more(
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

pub(super) fn render_kv_args(value: &Value) -> Option<String> {
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
