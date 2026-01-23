#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;

use super::TAG_MORE;
use super::util::{append_warnings_as_warnings, opt_str, truncate_line};

pub(super) fn render_branchmind_status_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let checkout = result
        .get("checkout")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let version = result
        .get("server")
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));
    let build = result
        .get("server")
        .and_then(|v| v.get("build_fingerprint"))
        .and_then(|v| v.as_str())
        .and_then(|s| opt_str(Some(s)));

    let last_event = result
        .get("last_task_event")
        .or_else(|| result.get("last_event"))
        .and_then(|v| v.get("event_id"))
        .and_then(|v| v.as_str());
    let last_doc_seq = result
        .get("last_doc_entry")
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_u64());
    let last_doc_doc = result
        .get("last_doc_entry")
        .and_then(|v| v.get("doc"))
        .and_then(|v| v.as_str());

    let mut lines = Vec::new();
    let mut state = format!("ready checkout={checkout}");
    if let Some(version) = version {
        state.push_str(" version=");
        state.push_str(version);
    }
    if let Some(build) = build {
        state.push_str(" build=");
        state.push_str(build);
    }
    let mut last_parts = Vec::new();
    if let Some(event) = opt_str(last_event) {
        last_parts.push(format!("task_event={event}"));
    }
    if let (Some(doc), Some(seq)) = (opt_str(last_doc_doc), last_doc_seq) {
        last_parts.push(format!("doc={doc}@{seq}"));
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

pub(super) fn render_branchmind_macro_branch_note_lines(
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

pub(super) fn render_branchmind_anchors_list_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let has_more = result
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let anchors = result
        .get("anchors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(format!("anchors count={} more={has_more}", anchors.len()));

    let show_limit = 20usize;
    for a in anchors.iter().take(show_limit) {
        let id = a.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
        let status = a.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let title = a.get("title").and_then(|v| v.as_str()).unwrap_or("-");
        let title = truncate_line(title, 72);
        lines.push(format!("- {id} kind={kind} status={status} title={title}"));
    }
    if anchors.len() > show_limit {
        lines.push(format!("{TAG_MORE}: Increase limit to list more anchors."));
    }

    // Safe next steps.
    lines.push("anchor_snapshot anchor=\"a:<slug>\"".to_string());
    lines.push("anchors_export format=mermaid".to_string());
    lines.push("anchors_lint".to_string());
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_branchmind_anchor_snapshot_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let anchor = result.get("anchor").unwrap_or(&Value::Null);
    let anchor_id = anchor.get("id").and_then(|v| v.as_str()).unwrap_or("-");
    let title = anchor.get("title").and_then(|v| v.as_str()).unwrap_or("-");
    let kind = anchor.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
    let status = anchor.get("status").and_then(|v| v.as_str()).unwrap_or("-");

    let tasks = result
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let cards = result
        .get("cards")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(format!(
        "anchor {anchor_id} kind={kind} status={status} title={} ",
        truncate_line(title, 80)
    ));

    if !tasks.is_empty() {
        let mut parts = Vec::new();
        for task in tasks.iter().take(3) {
            let id = task.get("task").and_then(|v| v.as_str()).unwrap_or("-");
            let status = task
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            parts.push(format!("{id}({status})"));
        }
        let mut line = format!("tasks {}", parts.join(" "));
        if tasks.len() > 3 {
            line.push_str(" ...");
        }
        lines.push(line);

        if let Some(task_id) = tasks
            .first()
            .and_then(|t| t.get("task"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "-")
        {
            lines.push(format!("tasks_snapshot task={task_id}"));
        }
    }

    for card in cards.iter().take(10) {
        let id = card.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("note");
        let label = card
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| card.get("text").and_then(|v| v.as_str()))
            .unwrap_or("-");
        let label = truncate_line(label, 84);
        lines.push(format!("- {ty} {id}: {label}"));
    }
    if cards.len() > 10 {
        lines.push(format!("{TAG_MORE}: include_drafts=true limit=..."));
    }

    lines.push(format!(
        "macro_anchor_note anchor=\"{anchor_id}\" content=\"...\" visibility=canon pin=true"
    ));
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_branchmind_macro_anchor_note_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let anchor = result.get("anchor").unwrap_or(&Value::Null);
    let anchor_id = anchor.get("id").and_then(|v| v.as_str()).unwrap_or("-");
    let created = anchor
        .get("created")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let note_id = result
        .get("note")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("-");

    let mut lines = Vec::new();
    if created {
        lines.push(format!("anchor {anchor_id} created | note {note_id}"));
    } else {
        lines.push(format!("anchor {anchor_id} updated | note {note_id}"));
    }
    lines.push(format!("anchor_snapshot anchor=\"{anchor_id}\""));
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}

pub(super) fn render_branchmind_anchors_export_lines(
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
    let result = response.get("result").unwrap_or(&Value::Null);
    let format = result.get("format").and_then(|v| v.as_str()).unwrap_or("-");
    let count = result
        .get("anchors_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let text = result.get("text").and_then(|v| v.as_str()).unwrap_or("");

    let mut lines = Vec::new();
    lines.push(format!("export format={format} anchors={count}"));
    if !text.trim().is_empty() {
        lines.push(text.trim_end().to_string());
    }
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}
