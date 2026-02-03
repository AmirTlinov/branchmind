#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Envelope, OpResponse,
    Safety, SchemaSource, Stability, Tier, ToolName, name_to_cmd_segments,
};
use serde_json::{Value, json};

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // Curated v1 docs portal ops (golden aliases in tools/list):
    // - list/show/diff/merge
    register_handler(specs, "docs.list", "docs_list", Some("list"));
    register_handler(specs, "docs.show", "show", Some("show"));
    register_handler(specs, "docs.diff", "diff", Some("diff"));
    register_handler(specs, "docs.merge", "merge", Some("merge"));

    // Long-tail docs-ish tools (call-only).
    for handler_name in [
        "transcripts_search",
        "transcripts_open",
        "transcripts_digest",
        "export",
    ] {
        let cmd = format!("docs.{}", name_to_cmd_segments(handler_name));
        match handler_name {
            "transcripts_open" => register_handler_with_hook(
                specs,
                &cmd,
                handler_name,
                None,
                Some(handle_transcripts_open),
            ),
            "transcripts_digest" => register_handler_with_hook(
                specs,
                &cmd,
                handler_name,
                None,
                Some(handle_transcripts_digest),
            ),
            _ => register_handler(specs, &cmd, handler_name, None),
        }
    }
}

fn register_handler(
    specs: &mut Vec<CommandSpec>,
    cmd: &str,
    handler_name: &str,
    op_alias: Option<&str>,
) {
    register_handler_with_hook(specs, cmd, handler_name, op_alias, None);
}

fn register_handler_with_hook(
    specs: &mut Vec<CommandSpec>,
    cmd: &str,
    handler_name: &str,
    op_alias: Option<&str>,
    handler: Option<fn(&mut crate::McpServer, &Envelope) -> OpResponse>,
) {
    let mut op_aliases = Vec::<String>::new();
    if let Some(op) = op_alias {
        op_aliases.push(op.to_string());
    }

    specs.push(CommandSpec {
        cmd: cmd.to_string(),
        domain_tool: ToolName::DocsOps,
        tier: Tier::Advanced,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: format!("#{cmd}"),
        },
        safety: Safety {
            destructive: handler_name == "merge",
            confirm_level: if handler_name == "merge" {
                ConfirmLevel::Soft
            } else {
                ConfirmLevel::None
            },
            idempotent: handler_name != "merge",
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Handler,
        op_aliases,
        handler_name: Some(handler_name.to_string()),
        handler,
    });
}

fn handle_transcripts_digest(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "transcripts_digest", env.args.clone())
            .unwrap_or_else(|| {
                crate::ai_error("INTERNAL_ERROR", "transcripts_digest dispatch failed")
            });
    let mut resp =
        crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp);

    if resp.error.is_none() {
        let warning_codes = resp
            .warnings
            .iter()
            .filter_map(|w| w.get("code").and_then(|v| v.as_str()))
            .collect::<Vec<_>>();
        let needs_retry = warning_codes
            .iter()
            .any(|c| *c == "TRANSCRIPTS_SCAN_TRUNCATED" || *c == "TRANSCRIPTS_MAX_FILES_REACHED");

        if needs_retry
            && !resp.actions.iter().any(|a| {
                a.args
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .is_some_and(|cmd| cmd == "docs.transcripts.digest")
            })
        {
            let args_obj = env.args.as_object().cloned().unwrap_or_default();
            let mode = args_obj
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("summary");
            let max_files = args_obj
                .get("max_files")
                .and_then(|v| v.as_u64())
                .unwrap_or(720) as usize;
            let max_bytes_total = args_obj
                .get("max_bytes_total")
                .and_then(|v| v.as_u64())
                .unwrap_or(16 * 1024 * 1024) as usize;
            let max_items = args_obj
                .get("max_items")
                .and_then(|v| v.as_u64())
                .unwrap_or(6) as usize;
            let bump_bytes = if max_bytes_total < 64 * 1024 * 1024 {
                64 * 1024 * 1024
            } else {
                (max_bytes_total.saturating_mul(2)).min(256 * 1024 * 1024)
            };
            let bump_files = if max_files < 120 {
                120
            } else {
                (max_files.saturating_mul(2)).min(2048)
            };

            let mut inner = serde_json::Map::new();
            if let Some(root_dir) = args_obj.get("root_dir").and_then(|v| v.as_str()) {
                inner.insert("root_dir".to_string(), Value::String(root_dir.to_string()));
            }
            if let Some(cwd_prefix) = args_obj.get("cwd_prefix").and_then(|v| v.as_str()) {
                inner.insert(
                    "cwd_prefix".to_string(),
                    Value::String(cwd_prefix.to_string()),
                );
            }
            inner.insert("mode".to_string(), Value::String(mode.to_string()));
            inner.insert("max_items".to_string(), json!(max_items));
            inner.insert("max_files".to_string(), json!(bump_files));
            inner.insert("max_bytes_total".to_string(), json!(bump_bytes));
            if let Some(max_chars) = args_obj.get("max_chars").and_then(|v| v.as_u64()) {
                inner.insert("max_chars".to_string(), json!(max_chars));
            }

            resp.actions.push(Action {
                action_id: "retry.docs.transcripts.digest.bump_budget".to_string(),
                priority: ActionPriority::Low,
                tool: ToolName::DocsOps.as_str().to_string(),
                args: json!({
                    "workspace": env.workspace,
                    "op": "call",
                    "cmd": "docs.transcripts.digest",
                    "args": Value::Object(inner),
                    "budget_profile": "default",
                    "view": "compact"
                }),
                why: "Повторить digest с большим scan budget (bounded).".to_string(),
                risk: "Низкий".to_string(),
            });

            if mode == "summary" {
                let mut last_args = resp
                    .actions
                    .last()
                    .and_then(|a| a.args.as_object())
                    .cloned()
                    .unwrap_or_default();
                if let Some(env_args) = last_args.get_mut("args").and_then(|v| v.as_object_mut()) {
                    env_args.insert("mode".to_string(), Value::String("last".to_string()));
                }
                resp.actions.push(Action {
                    action_id: "retry.docs.transcripts.digest.mode_last".to_string(),
                    priority: ActionPriority::Low,
                    tool: ToolName::DocsOps.as_str().to_string(),
                    args: Value::Object(last_args),
                    why: "Попробовать mode='last' для быстрой ориентации.".to_string(),
                    risk: "Низкий".to_string(),
                });
            }
        }
    }

    resp
}

fn handle_transcripts_open(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "transcripts_open", env.args.clone())
            .unwrap_or_else(|| {
                crate::ai_error("INTERNAL_ERROR", "transcripts_open dispatch failed")
            });
    let mut resp =
        crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp);

    if resp.error.is_none()
        && !resp.actions.iter().any(|a| {
            a.args
                .get("cmd")
                .and_then(|v| v.as_str())
                .is_some_and(|cmd| cmd == "think.idea.branch.create")
        })
    {
        let workspace = env.workspace.as_deref().unwrap_or("");
        let capture_meta = build_transcript_capture_meta(server, workspace, &resp.result);
        let capture_content = build_transcript_capture_content(&resp.result);

        resp.actions.push(Action {
            action_id: "capture.transcript.personal".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": env.workspace,
                "op": "call",
                "cmd": "think.idea.branch.create",
                "args": {
                    "title": "Transcript capture",
                    "format": "text",
                    "meta": capture_meta,
                    "content": capture_content
                },
                "budget_profile": "default",
                "view": "compact"
            }),
            why: "Зафиксировать окно транскрипта как идею/капсулу (личная полоса).".to_string(),
            risk: "Низкий".to_string(),
        });

        resp.actions.push(Action {
            action_id: "capture.transcript.shared".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": env.workspace,
                "op": "call",
                "cmd": "think.idea.branch.create",
                "args": {
                    "agent_id": Value::Null,
                    "title": "Transcript capture (shared)",
                    "format": "text",
                    "meta": capture_meta,
                    "content": capture_content
                },
                "budget_profile": "default",
                "view": "compact"
            }),
            why: "Зафиксировать окно транскрипта как идею/капсулу (shared lane).".to_string(),
            risk: "Низкий".to_string(),
        });
    }

    resp
}

fn build_transcript_capture_content(result: &Value) -> String {
    let ref_obj = result.get("ref").cloned().unwrap_or_else(|| json!({}));
    let path = ref_obj
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("<transcript>");
    let line = ref_obj.get("line").and_then(|v| v.as_u64());
    let byte = ref_obj.get("byte").and_then(|v| v.as_u64());

    let mut header = format!("[Transcript] {path}");
    if let Some(line) = line {
        header.push_str(&format!(" (line {line})"));
    } else if let Some(byte) = byte {
        header.push_str(&format!(" (byte {byte})"));
    }

    let mut out = String::new();
    out.push_str(&header);
    out.push('\n');
    if let Some(session) = result.get("session").and_then(|v| v.as_object()) {
        let id = session.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let ts = session.get("ts").and_then(|v| v.as_str()).unwrap_or("");
        let cwd = session.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
        if !id.is_empty() || !ts.is_empty() || !cwd.is_empty() {
            out.push_str(&format!("session: id={id} ts={ts} cwd={cwd}\n"));
        }
    }
    if let Some(project) = result.get("project").and_then(|v| v.as_object()) {
        let name = project.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let id = project.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let conf = project
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !name.is_empty() || !id.is_empty() {
            out.push_str(&format!("project: {name} ({id}) confidence={conf}\n"));
        }
    }
    out.push_str("---\n");

    if let Some(entries) = result.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            let role = entry.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let ts = entry.get("ts").and_then(|v| v.as_str()).unwrap_or("");
            let text = entry.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if role.is_empty() && text.is_empty() {
                continue;
            }
            if !ts.is_empty() {
                out.push_str(&format!("[{role}] {ts}\n{text}\n\n"));
            } else {
                out.push_str(&format!("[{role}]\n{text}\n\n"));
            }
        }
    }

    out.trim().to_string()
}

fn build_transcript_capture_meta(
    server: &mut crate::McpServer,
    workspace: &str,
    result: &Value,
) -> Value {
    let mut meta = json!({
        "source": { "kind": "transcripts", "tool": "docs.transcripts.open" },
        "transcript": {
            "ref": result.get("ref").cloned().unwrap_or_else(|| json!({})),
            "session": result.get("session").cloned().unwrap_or_else(|| json!({})),
            "project": result.get("project").cloned().unwrap_or_else(|| json!({}))
        }
    });

    let Ok(workspace_id) = crate::WorkspaceId::try_new(workspace.to_string()) else {
        return meta;
    };

    if let Ok(Some(focus)) = server.store.focus_get(&workspace_id)
        && focus.starts_with("TASK-")
        && let Ok(summary) = server.store.task_steps_summary(&workspace_id, &focus)
        && let Some(first_open) = summary.first_open
        && let Some(obj) = meta.as_object_mut()
    {
        obj.insert(
            "step".to_string(),
            json!({
                "task_id": focus,
                "step_id": first_open.step_id,
                "path": first_open.path
            }),
        );
    }

    meta
}
