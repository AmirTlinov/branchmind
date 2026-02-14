#![forbid(unsafe_code)]

use crate::ops::{QUICKSTART_DEFAULT_PORTAL, quickstart_curated_portals_joined};
use crate::*;
use serde_json::Value;

impl McpServer {
    pub(crate) fn tool_branchmind_help(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let text = render_help_text(max_chars);

        let mut resp = ai_ok("help", Value::String(text));
        if let Some(obj) = resp.as_object_mut() {
            // Return as raw text in MCP (no JSON pretty-print), because help is read-mostly
            // and should not waste context on an envelope.
            obj.insert("line_protocol".to_string(), Value::Bool(true));
        }
        resp
    }
}

fn render_help_text(max_chars: Option<usize>) -> String {
    let mut out = Vec::<String>::new();

    push_section(
        &mut out,
        "LEGEND",
        &[
            "State line: plain (no tag).",
            "Next actions: plain command lines (no COMMAND: prefix).",
            "ERROR: typed error + fix hint (1 line).",
            "WARNING: typed heads-up + fix hint (optional).",
            "MORE: continuation marker (pagination cursors).",
        ],
    );

    let daily_lines = vec![
        "v1 surface: 10 portal tools (status/open/workspace/tasks/jobs/think/graph/vcs/docs/system)."
            .to_string(),
        "Golden path: status → tasks(op=call cmd=tasks.macro.start) → tasks(op=call cmd=tasks.snapshot)."
            .to_string(),
        "When stuck: follow actions[] (copy/paste-ready).".to_string(),
        "Workspace: workspace=<id> or workspace=<absolute path> (paths are auto-mapped/bound to a stable id)."
            .to_string(),
        "Bindings: workspace(op=list) shows workspaces + bound_path (transparency for path→id mapping)."
            .to_string(),
        format!(
            "Discovery: system(op=tools.list) or system(op=quickstart args={{portal:\"{}\"}}) or system(op=tutorial) (copy/paste recipes).",
            QUICKSTART_DEFAULT_PORTAL
        ),
        format!(
            "Quickstart portals: {}.",
            quickstart_curated_portals_joined()
        ),
        "Schemas: system(op=schema.list args={portal:\"tasks\"}) → system(op=schema.get args={cmd:\"tasks.snapshot\"})."
            .to_string(),
        "Cmd fallback: system(op=cmd.list args={q:\"tasks.\", limit:50}) (then schema.get).".to_string(),
        "Budget knobs: budget_profile=portal|default|audit; portal_view=compact|smart|audit (content stays bounded)."
            .to_string(),
    ];
    push_section_owned(&mut out, "DAILY", &daily_lines);

    push_section(
        &mut out,
        "PROOF",
        &[
            "Preferred receipts: CMD: ... (what you ran) + LINK: ... (CI/artifact/log).",
            "Fast path: tasks(op=call cmd=tasks.macro.close.step) accepts proof/proof_input and returns a fresh resume.",
            "Auto-normalization (proof_input/proof): URL lines become LINK; strong shell command lines become CMD; path-like lines become FILE; everything else becomes NOTE (does not count as proof).",
            "Markdown bullets are accepted in proof lines (e.g. '- LINK: ...').",
            "URL-like attachments count as LINK for the soft PROOF_WEAK lint.",
            "Soft lint: missing CMD or LINK emits WARNING: PROOF_WEAK (does not block).",
            "Hard gate: proof-required steps fail with ERROR: PROOF_REQUIRED (retry with proof).",
        ],
    );

    push_section(
        &mut out,
        "DELEGATION",
        &[
            "Delegate work as jobs (JOB-*). BranchMind tracks; runners execute out-of-process.",
            "Create: tasks(op=call cmd=tasks.macro.delegate) (task + job), or jobs(op=create) for a raw job record.",
            "Inbox: jobs(op=radar). Open details via open(id=JOB-*@seq) or jobs(op=open).",
            "Wait: jobs(op=call cmd=jobs.wait) (bounded by timeout_ms; retry in short loops).",
            "Proof: jobs(op=call cmd=jobs.proof.attach) can attach job artifacts as step evidence.",
        ],
    );

    push_section(
        &mut out,
        "ANCHORS",
        &[
            "Anchors are meaning coordinates: a:<slug> (not file paths).",
            "Recall-first: think(op=knowledge.recall args={anchor:\"a:<slug>\", limit:12}).",
            "To discover anchor commands: system(op=cmd.list args={q:\"anchor.\"}) and then system(op=schema.get ...).",
        ],
    );

    let mut text = out.join("\n");
    if let Some(limit) = max_chars {
        let (limit, _clamped) = clamp_budget_max(limit);
        if text.len() > limit {
            // Keep truncation deterministic and byte-safe.
            let suffix = "...";
            let budget = limit.saturating_sub(suffix.len());
            text = truncate_string_bytes(&text, budget) + suffix;
        }
    }
    text
}

fn push_section(out: &mut Vec<String>, name: &str, lines: &[&str]) {
    let mut body = Vec::<String>::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        body.push(trimmed.to_string());
    }
    if body.is_empty() {
        return;
    }

    out.push(format!("[{name}]"));
    out.extend(body);
}

fn push_section_owned(out: &mut Vec<String>, name: &str, lines: &[String]) {
    let mut body = Vec::<String>::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        body.push(trimmed.to_string());
    }
    if body.is_empty() {
        return;
    }

    out.push(format!("[{name}]"));
    out.extend(body);
}
