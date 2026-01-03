#![forbid(unsafe_code)]

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

    push_section(
        &mut out,
        "DAILY",
        &[
            "Golden path (core): status → tasks_macro_start → tasks_snapshot.",
            "Daily add-ons: tasks_macro_close_step, macro_branch_note.",
            "Progressive disclosure: tools/list toolset=daily|full reveals more tools when needed.",
        ],
    );

    push_section(
        &mut out,
        "PROOF",
        &[
            "Preferred receipts: CMD: ... (what you ran) + LINK: ... (CI/artifact/log).",
            "Proof shortcut: pass proof as string/array/object to tasks_macro_close_step.",
            "Auto-normalization: URL lines become LINK; other lines become CMD.",
            "Soft lint: missing CMD or LINK emits WARNING: PROOF_WEAK (does not block).",
            "Hard gate: proof-required steps fail with ERROR: PROOF_REQUIRED (retry with proof).",
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
