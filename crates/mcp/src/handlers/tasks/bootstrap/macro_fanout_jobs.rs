#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const DEFAULT_JOB_KIND: &str = "codex_cli";
const DEFAULT_JOB_PRIORITY: &str = "MEDIUM";
const MAX_FANOUT_JOBS: usize = 10;
const DEFAULT_JOB_SKILL_MAX_CHARS: usize = 1200;

fn default_skill_profile_for_job_kind(kind: &str) -> &'static str {
    let lowered = kind.trim().to_ascii_lowercase();
    if lowered.contains("research") {
        return "research";
    }
    "strict"
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::<String>::new();
    for v in values {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            continue;
        }
        set.insert(trimmed.to_string());
    }
    set.into_iter().collect()
}

impl McpServer {
    pub(crate) fn tool_tasks_macro_fanout_jobs(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        if kind != TaskKind::Task {
            return ai_error_with(
                "INVALID_INPUT",
                "macro_fanout_jobs requires a task target",
                Some("Set task focus or pass task=TASK-... to split work into per-anchor jobs."),
                Vec::new(),
            );
        }
        let task_id = target_id;

        let task_title = match self.store.get_task(&workspace, &task_id) {
            Ok(Some(task)) => task.title,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let prompt_base = match require_string(args_obj, "prompt") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let title_prefix = match optional_string(args_obj, "title_prefix") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let job_kind = match optional_string(args_obj, "job_kind") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_JOB_KIND.to_string()),
            Err(resp) => return resp,
        };
        let job_priority = match optional_string(args_obj, "job_priority") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_JOB_PRIORITY.to_string()),
            Err(resp) => return resp,
        };

        let anchors = match optional_string_array(args_obj, "anchors") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(anchors) = anchors else {
            return ai_error("INVALID_INPUT", "anchors is required");
        };

        let anchors = normalize_id_list(anchors);
        if anchors.is_empty() {
            return ai_error("INVALID_INPUT", "anchors must not be empty");
        }
        if anchors.len() > MAX_FANOUT_JOBS {
            return ai_error(
                "INVALID_INPUT",
                &format!(
                    "anchors exceeds max fanout ({}); fix: split into multiple calls",
                    MAX_FANOUT_JOBS
                ),
            );
        }

        let mut warnings = Vec::<Value>::new();
        let mut jobs = Vec::<Value>::new();

        for anchor_id in anchors.iter() {
            let mut derived_title = match title_prefix
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(prefix) => format!("{prefix} — {anchor_id}"),
                None => format!("{anchor_id} — {task_title}"),
            };
            derived_title = truncate_string(&redact_text(&derived_title), 200);

            let mut derived_prompt = prompt_base.trim_end().to_string();
            derived_prompt.push_str("\n\nCONTEXT:\n");
            derived_prompt.push_str(&format!("- task: {task_id}\n"));
            derived_prompt.push_str(&format!("- anchor: {anchor_id}\n"));
            derived_prompt.push_str("\nPROTOCOL:\n");
            derived_prompt.push_str("- report progress via tasks_jobs_report (short, no logs)\n");
            derived_prompt.push_str(
                "- land findings as durable artifacts and reference them by stable ids\n",
            );

            match self.store.job_create(
                &workspace,
                bm_storage::JobCreateRequest {
                    title: derived_title,
                    prompt: derived_prompt,
                    kind: job_kind.clone(),
                    priority: job_priority.clone(),
                    task_id: Some(task_id.clone()),
                    anchor_id: Some(anchor_id.clone()),
                    meta_json: serde_json::to_string(&json!({
                        "skill_profile": default_skill_profile_for_job_kind(&job_kind),
                        "skill_max_chars": DEFAULT_JOB_SKILL_MAX_CHARS,
                    }))
                    .ok(),
                },
            ) {
                Ok(created) => {
                    let created_ref = format!("{}@{}", created.job.id, created.created_event.seq);
                    jobs.push(json!({
                        "job_id": created.job.id,
                        "anchor": anchor_id,
                        "created_ref": created_ref
                    }));
                }
                Err(StoreError::InvalidInput(msg)) => {
                    warnings.push(warning("JOB_CREATE_FAILED", "failed to create job", msg));
                }
                Err(err) => {
                    warnings.push(warning(
                        "JOB_CREATE_FAILED",
                        "failed to create job",
                        format_store_error(err).as_str(),
                    ));
                }
            }
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "task_id": task_id,
            "count": jobs.len(),
            "jobs": jobs
        });
        if warnings.is_empty() {
            ai_ok("tasks_macro_fanout_jobs", result)
        } else {
            ai_ok_with_warnings("tasks_macro_fanout_jobs", result, warnings, Vec::new())
        }
    }
}
