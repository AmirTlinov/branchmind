#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

const DEFAULT_TASK_TEMPLATE: &str = "principal-task";
const DEFAULT_ANCHOR_KIND: &str = "component";
const DEFAULT_JOB_SKILL_PROFILE: &str = "strict";
const DEFAULT_JOB_SKILL_MAX_CHARS: usize = 1200;

fn suggested_anchor_title(task_title: &str) -> Option<String> {
    let title = task_title.trim();
    if title.is_empty() {
        return None;
    }
    // Heuristic: prefer the "prefix" before ':' as an anchor title candidate.
    // Example: "Storage: fix migrations" -> "Storage".
    if let Some((head, _)) = title.split_once(':') {
        let head = head.trim();
        if !head.is_empty() {
            return Some(truncate_string(&redact_text(head), 80));
        }
    }
    Some(truncate_string(&redact_text(title), 80))
}

fn derive_anchor_id_from_title(title: &str) -> String {
    // Deterministic, ascii-only slugify for `a:<slug>`:
    // - lowercased
    // - non-alnum => '-'
    // - collapse/trim '-'
    // - max 64 chars (slug)
    let raw = title.trim();
    if raw.is_empty() {
        return "a:core".to_string();
    }

    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 64 {
            break;
        }
    }

    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        return "a:core".to_string();
    }

    // Ensure the slug starts with [a-z0-9] after trimming.
    let slug = slug
        .chars()
        .skip_while(|c| !c.is_ascii_alphanumeric())
        .take(64)
        .collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return "a:core".to_string();
    }

    format!("a:{slug}")
}

fn anchor_exists_or_resolves(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    id: &str,
) -> Result<bool, Value> {
    match server.store.anchor_get(
        workspace,
        bm_storage::AnchorGetRequest { id: id.to_string() },
    ) {
        Ok(Some(_)) => return Ok(true),
        Ok(None) => {}
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }

    match server.store.anchor_resolve_id(workspace, id) {
        Ok(Some(_canonical)) => Ok(true),
        Ok(None) => Ok(false),
        Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}

impl McpServer {
    pub(crate) fn tool_tasks_macro_delegate(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let mut patched_args = args_obj.clone();
        let workspace = match require_workspace(&patched_args) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let workspace_label = workspace.as_str();

        let task_title = match require_string(args_obj, "task_title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let description = match optional_string(args_obj, "description") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let view = match optional_string(args_obj, "view") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let resume_max_chars = match optional_usize(args_obj, "resume_max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs = match optional_bool(args_obj, "refs") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let create_job = match optional_bool(args_obj, "job") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let job_kind = match optional_string(args_obj, "job_kind") {
            Ok(v) => v.unwrap_or_else(|| "codex_cli".to_string()),
            Err(resp) => return resp,
        };
        let job_priority = match optional_string(args_obj, "job_priority") {
            Ok(v) => v.unwrap_or_else(|| "MEDIUM".to_string()),
            Err(resp) => return resp,
        };

        // Principal defaults: make delegation "strict by default" and keep the input low-syntax.
        if !patched_args.contains_key("template") {
            patched_args.insert(
                "template".to_string(),
                Value::String(DEFAULT_TASK_TEMPLATE.to_string()),
            );
        }
        if !patched_args.contains_key("reasoning_mode") {
            patched_args.insert(
                "reasoning_mode".to_string(),
                Value::String("strict".to_string()),
            );
        }
        // Ensure portal tool args are forwarded to the underlying resume for a consistent view.
        if let Some(view) = view.as_deref() {
            patched_args.insert("view".to_string(), Value::String(view.to_string()));
        }
        if refs {
            patched_args.insert("refs".to_string(), Value::Bool(true));
        }
        if let Some(max_chars) = resume_max_chars {
            patched_args.insert(
                "resume_max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        // Step 1: create task + steps (principal template) and get a baseline resume.
        let start = self.tool_tasks_macro_start(Value::Object(patched_args.clone()));
        if !start
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return start;
        }

        let task_id = match start
            .get("result")
            .and_then(|v| v.get("task_id"))
            .and_then(|v| v.as_str())
        {
            Some(v) => v.to_string(),
            None => return ai_error("STORE_ERROR", "tasks_macro_start result missing task_id"),
        };
        let plan_id = start
            .get("result")
            .and_then(|v| v.get("plan_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut warnings = Vec::new();
        if let Some(w) = start.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        // Step 2: pick an anchor for this initiative and seed a pinned cockpit note.
        let anchor_override = match optional_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let anchor_kind = match optional_string(args_obj, "anchor_kind") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_ANCHOR_KIND.to_string()),
            Err(resp) => return resp,
        };
        let cockpit_override = match optional_string(args_obj, "cockpit") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut anchor_title =
            suggested_anchor_title(&task_title).unwrap_or_else(|| "Core".to_string());
        let requested_anchor_id = anchor_override
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|raw| {
                let raw_lc = raw.to_ascii_lowercase();
                if raw_lc.starts_with("a:") {
                    raw_lc
                } else {
                    // DX: allow passing a human-friendly anchor label ("Storage") and derive
                    // a stable anchor id from it.
                    anchor_title = truncate_string(&redact_text(raw), 80);
                    derive_anchor_id_from_title(&anchor_title)
                }
            })
            .unwrap_or_else(|| derive_anchor_id_from_title(&anchor_title));

        // If the anchor doesn't exist (and isn't an alias), provide title/kind so it can be created.
        let needs_create = match anchor_exists_or_resolves(self, &workspace, &requested_anchor_id) {
            Ok(v) => !v,
            Err(resp) => return resp,
        };

        let cockpit_text = cockpit_override.unwrap_or_else(|| {
            let template = render_note_template("initiative", Some(&task_title))
                .unwrap_or_else(|| task_title.clone());
            let mut header = format!("COCKPIT — {}", task_title.trim());
            if let Some(description) = description
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                header.push('\n');
                header.push_str(description);
            }
            format!("{header}\n\n{}", template.trim_end())
        });

        let mut macro_args = serde_json::Map::new();
        macro_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        macro_args.insert(
            "anchor".to_string(),
            Value::String(requested_anchor_id.clone()),
        );
        macro_args.insert("target".to_string(), Value::String(task_id.clone()));
        macro_args.insert("content".to_string(), Value::String(cockpit_text));
        macro_args.insert("card_type".to_string(), Value::String("frame".to_string()));
        macro_args.insert("visibility".to_string(), Value::String("canon".to_string()));
        macro_args.insert("pin".to_string(), Value::Bool(true));
        if needs_create {
            macro_args.insert("title".to_string(), Value::String(anchor_title.clone()));
            macro_args.insert("kind".to_string(), Value::String(anchor_kind));
        }

        let cockpit_resp = self.tool_branchmind_macro_anchor_note(Value::Object(macro_args));
        let cockpit_ok = cockpit_resp
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let cockpit = if cockpit_ok {
            let anchor_id = cockpit_resp
                .get("result")
                .and_then(|v| v.get("anchor"))
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or(&requested_anchor_id)
                .to_string();
            let card_id = cockpit_resp
                .get("result")
                .and_then(|v| v.get("note"))
                .and_then(|v| v.get("card_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            json!({ "anchor_id": anchor_id, "card_id": card_id })
        } else {
            let message = cockpit_resp
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Failed to seed cockpit");
            warnings.push(warning(
                "COCKPIT_SEED_FAILED",
                message,
                "Run macro_anchor_note manually to create the cockpit and retry.",
            ));
            json!({
                "ok": false,
                "error": cockpit_resp.get("error").cloned().unwrap_or(Value::Null),
                "anchor_id": requested_anchor_id
            })
        };

        // Step 2.5: create a delegation job record (execution remains external).
        //
        // This is the bridge that lets an external runner claim the work and report progress
        // without losing narrative across restarts.
        let job = if create_job {
            let anchor_id = cockpit
                .get("anchor_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| s.starts_with("a:") && !s.is_empty())
                .unwrap_or_else(|| requested_anchor_id.clone());

            let mut prompt = String::new();
            prompt.push_str("Delegated job (runner executes out-of-process).\n");
            prompt.push_str("Record durable results as BranchMind artifacts (cards/notes/evidence) and reference them by stable ids.\n\n");
            prompt.push_str("WORK:\n");
            prompt.push_str(&format!("- task: {task_id}\n"));
            prompt.push_str(&format!("- anchor: {anchor_id}\n"));
            prompt.push_str(&format!(
                "- title: {}\n",
                truncate_string(&redact_text(&task_title), 180)
            ));
            if let Some(description) = description
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                prompt.push_str(&format!(
                    "- goal: {}\n",
                    truncate_string(&redact_text(description), 400)
                ));
            }
            prompt.push_str("\nOUTPUT (expected):\n");
            prompt.push_str("- progress updates (short, no logs)\n");
            prompt.push_str("- final summary (what changed + what was verified)\n");
            prompt.push_str("- stable refs: CARD-... / notes@seq / TASK-...\n");

            match self.store.job_create(
                &workspace,
                bm_storage::JobCreateRequest {
                    title: task_title.clone(),
                    prompt,
                    kind: job_kind.clone(),
                    priority: job_priority.clone(),
                    task_id: Some(task_id.clone()),
                    anchor_id: Some(anchor_id),
                    meta_json: serde_json::to_string(&json!({
                        "skill_profile": DEFAULT_JOB_SKILL_PROFILE,
                        "skill_max_chars": DEFAULT_JOB_SKILL_MAX_CHARS,
                    }))
                    .ok(),
                },
            ) {
                Ok(created) => json!({
                    "job_id": created.job.id,
                    "status": created.job.status,
                    "created_at_ms": created.job.created_at_ms,
                    "updated_at_ms": created.job.updated_at_ms
                }),
                Err(StoreError::InvalidInput(msg)) => {
                    warnings.push(warning(
                        "JOB_CREATE_FAILED",
                        "failed to create delegation job",
                        msg,
                    ));
                    Value::Null
                }
                Err(err) => {
                    warnings.push(warning(
                        "JOB_CREATE_FAILED",
                        "failed to create delegation job",
                        &format_store_error(err),
                    ));
                    Value::Null
                }
            }
        } else {
            Value::Null
        };

        // Step 3: rerun resume so the pinned cockpit is visible immediately.
        let mut resume_args = serde_json::Map::new();
        resume_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        resume_args.insert("task".to_string(), Value::String(task_id.clone()));
        if let Some(agent_id) = agent_id.as_deref() {
            resume_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
        }
        resume_args.insert(
            "view".to_string(),
            Value::String(view.unwrap_or_else(|| "smart".to_string())),
        );
        if let Some(max_chars) = resume_max_chars {
            resume_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        let resume = self.tool_tasks_resume_super(Value::Object(resume_args));
        if !resume
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return resume;
        }

        if let Some(w) = cockpit_resp.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }
        if let Some(w) = resume.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let mut result = json!({
            "task_id": task_id,
            "task_qualified_id": format!("{workspace_label}:{task_id}"),
            "cockpit": cockpit,
            "job": job,
            "resume": resume.get("result").cloned().unwrap_or(Value::Null)
        });
        if let Some(plan_id) = plan_id.as_ref()
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("plan_id".to_string(), Value::String(plan_id.clone()));
            obj.insert(
                "plan_qualified_id".to_string(),
                Value::String(format!("{workspace_label}:{plan_id}")),
            );
        }

        if warnings.is_empty() {
            ai_ok("tasks_macro_delegate", result)
        } else {
            ai_ok_with_warnings("tasks_macro_delegate", result, warnings, Vec::new())
        }
    }
}
