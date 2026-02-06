#![forbid(unsafe_code)]

use crate::Toolset;
use serde_json::Value;
use std::collections::HashMap;

mod util;
use util::*;

mod actions;
mod branchmind;
mod generic;
mod tasks;

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
            tasks::render_tasks_macro_start_lines(args, response, toolset, omit_workspace)
        }
        "tasks_macro_delegate" => {
            tasks::render_tasks_macro_delegate_lines(args, response, toolset, omit_workspace)
        }
        "tasks_macro_close_step" => {
            tasks::render_tasks_macro_close_step_lines(args, response, toolset, omit_workspace)
        }
        "tasks_snapshot" => {
            tasks::render_tasks_snapshot_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_list" => {
            tasks::render_tasks_jobs_list_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_radar" => {
            tasks::render_tasks_jobs_radar_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_open" => {
            tasks::render_tasks_jobs_open_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_tail" => {
            tasks::render_tasks_jobs_tail_lines(args, response, toolset, omit_workspace)
        }
        "tasks_jobs_message" => {
            tasks::render_tasks_jobs_message_lines(args, response, toolset, omit_workspace)
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

fn portalize_handler_name_call(
    handler_name: &str,
    args_value: Option<&Value>,
    outer_workspace: Option<&str>,
    omit_workspace: bool,
) -> Option<String> {
    let handler_name = handler_name.trim();
    if handler_name.is_empty() {
        return None;
    }
    // Already a v1 portal tool (or a method name like tools/list) — render directly.
    if crate::tools_v1::is_v1_tool(handler_name) {
        let args_str = args_value.and_then(render_kv_args).unwrap_or_default();
        return Some(if args_str.is_empty() {
            handler_name.to_string()
        } else {
            format!("{handler_name} {args_str}")
        });
    }

    let registry = crate::ops::CommandRegistry::global();
    let spec = registry.find_by_handler_name(handler_name)?;

    let mut inner = args_value
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Portal UX: default checkpoint set when omitted (copy/paste-safe discipline).
    if handler_name == "tasks_macro_close_step" && !inner.contains_key("checkpoints") {
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
