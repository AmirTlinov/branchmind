#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAX_JOBS: usize = 50;
const OPEN_EVENTS_LIMIT: usize = 50;

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

fn is_completion_kind(kind: &str) -> bool {
    matches!(kind, "completed" | "failed" | "canceled")
}

impl McpServer {
    pub(crate) fn tool_tasks_macro_merge_report(&mut self, args: Value) -> Value {
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
                "macro_merge_report requires a task target",
                Some(
                    "Set task focus or pass task=TASK-... to merge delegated job results into a single report.",
                ),
                Vec::new(),
            );
        }
        let task_id = target_id;

        let task_title = match self.store.get_task(&workspace, &task_id) {
            Ok(Some(task)) => task.title,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let jobs = match optional_string_array(args_obj, "jobs") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(jobs) = jobs else {
            return ai_error("INVALID_INPUT", "jobs is required");
        };
        let jobs = normalize_id_list(jobs);
        if jobs.is_empty() {
            return ai_error("INVALID_INPUT", "jobs must not be empty");
        }
        if jobs.len() > MAX_JOBS {
            return ai_error("INVALID_INPUT", "jobs exceeds max items");
        }

        let report_title = match optional_string(args_obj, "title") {
            Ok(v) => v
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| truncate_string(&redact_text(s), 140))
                .unwrap_or_else(|| {
                    format!(
                        "MERGE REPORT — {}",
                        truncate_string(&redact_text(&task_title), 120)
                    )
                }),
            Err(resp) => return resp,
        };
        let pin = match optional_bool(args_obj, "pin") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };

        let mut anchor_tags = BTreeSet::<String>::new();
        let mut job_rows = Vec::<Value>::new();

        let mut report_lines = Vec::<String>::new();
        report_lines.push(report_title.clone());
        report_lines.push(String::new());
        report_lines.push(format!("task: {task_id}"));
        report_lines.push(String::new());
        report_lines.push("jobs:".to_string());

        for job_id in jobs.iter() {
            let open = match self.store.job_open(
                &workspace,
                bm_storage::JobOpenRequest {
                    id: job_id.clone(),
                    include_prompt: false,
                    include_events: true,
                    include_meta: false,
                    max_events: OPEN_EVENTS_LIMIT,
                    before_seq: None,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let anchor = open
                .job
                .anchor_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if let Some(anchor) = anchor.as_deref()
                && anchor.starts_with("a:")
            {
                anchor_tags.insert(anchor.to_string());
            }

            let last_ref = open
                .events
                .first()
                .map(|e| format!("{}@{}", job_id, e.seq))
                .unwrap_or_else(|| job_id.clone());

            let completion = open
                .events
                .iter()
                .find(|e| is_completion_kind(e.kind.as_str()));
            let mut completion_refs = completion.map(|e| e.refs.clone()).unwrap_or_else(Vec::new);
            completion_refs.sort();

            let summary = open
                .job
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| truncate_string(&redact_text(s), 240));

            let missing_proof = open.job.status == "DONE" && completion_refs.is_empty();

            let mut line = format!("- {job_id} ({})", open.job.status);
            if let Some(anchor) = anchor.as_deref() {
                line.push_str(&format!(" anchor={anchor}"));
            }
            line.push_str(&format!(" last={last_ref}"));
            if !completion_refs.is_empty() {
                line.push_str(" refs=");
                line.push_str(&completion_refs.join(","));
            }
            if missing_proof {
                line.push_str(" MISSING_PROOF");
            }
            report_lines.push(line);
            if let Some(summary) = summary.as_deref() {
                report_lines.push(format!("  summary: {summary}"));
            }

            job_rows.push(json!({
                "job_id": job_id,
                "status": open.job.status,
                "anchor": anchor,
                "last_ref": last_ref,
                "completion_refs": completion_refs,
                "missing_proof": missing_proof
            }));
        }

        // Canonical next action: open the newest job event ref that needs attention.
        let next_open = job_rows
            .iter()
            .find(|j| {
                j.get("missing_proof")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .or_else(|| {
                job_rows
                    .iter()
                    .find(|j| j.get("status").and_then(|v| v.as_str()) == Some("RUNNING"))
            })
            .and_then(|j| j.get("last_ref").and_then(|v| v.as_str()))
            .map(|r| r.to_string());

        if let Some(next_open) = next_open {
            report_lines.push(String::new());
            report_lines.push("next:".to_string());
            report_lines.push(format!("- open id={next_open}"));
        }

        let mut tags = anchor_tags.into_iter().collect::<Vec<_>>();
        tags.sort();

        let card = json!({
            "type": "update",
            "title": report_title,
            "text": report_lines.join("\n"),
            "tags": tags
        });

        let think = self.tool_branchmind_think_card(json!({
            "workspace": workspace.as_str(),
            "target": task_id.as_str(),
            "card": card
        }));
        if !think
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return think;
        }
        let Some(card_id) = think
            .get("result")
            .and_then(|v| v.get("card_id"))
            .and_then(|v| v.as_str())
        else {
            return ai_error("STORE_ERROR", "think_card result missing card_id");
        };

        let published = self.tool_branchmind_think_publish(json!({
            "workspace": workspace.as_str(),
            "target": task_id.as_str(),
            "card_id": card_id,
            "pin": pin
        }));
        if !published
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return published;
        }
        let published_card_id = published
            .get("result")
            .and_then(|v| v.get("published_card_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("-")
            .to_string();

        ai_ok(
            "tasks_macro_merge_report",
            json!({
                "workspace": workspace.as_str(),
                "task_id": task_id.as_str(),
                "card_id": card_id,
                "published_card_id": published_card_id,
                "jobs": job_rows,
                "count": job_rows.len()
            }),
        )
    }
}
