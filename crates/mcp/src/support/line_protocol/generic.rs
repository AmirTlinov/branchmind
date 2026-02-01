#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;

use super::TAG_ERROR;
use super::actions::append_actions_as_commands_limited;
use super::util::{append_suggestions_as_commands_limited, append_warnings_as_warnings};

pub(super) fn render_generic_lines(
    _tool: &str,
    _args: &Value,
    response: &Value,
    _toolset: Toolset,
) -> String {
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
        let has_suggestions = response
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| !arr.is_empty());
        if has_suggestions {
            append_suggestions_as_commands_limited(&mut lines, response, 2);
        } else {
            append_actions_as_commands_limited(&mut lines, response, 2);
        }
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
    // Generic fallback: keep the next move portal-first. The v1 surface is always 10 tools,
    // so `tools/list toolset=full` is no longer a meaningful disclosure mechanism.
    lines.push("status".to_string());
    append_warnings_as_warnings(&mut lines, response);
    lines.join("\n")
}
