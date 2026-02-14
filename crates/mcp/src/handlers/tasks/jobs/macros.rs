#![forbid(unsafe_code)]

use super::*;
use serde_json::{Value, json};

fn require_non_empty_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Value> {
    let v = require_string(args_obj, key)?;
    let t = v.trim();
    if t.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(t.to_string())
}

fn ensure_has_proof_ref(refs: &[String]) -> bool {
    refs.iter().any(|r| {
        let t = r.trim_start();
        t.starts_with("LINK:") || t.starts_with("CMD:") || t.starts_with("FILE:")
    })
}

fn normalize_jobs_list_optional(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<Option<Vec<String>>, Value> {
    if let Some(job_v) = args_obj.get("job") {
        let Some(job_raw) = job_v.as_str() else {
            return Err(ai_error("INVALID_INPUT", "job: expected string"));
        };
        let job = job_raw.trim();
        if job.is_empty() {
            return Err(ai_error("INVALID_INPUT", "job must not be empty"));
        }
        return Ok(Some(vec![job.to_string()]));
    }

    if let Some(jobs_v) = args_obj.get("jobs") {
        let Some(arr) = jobs_v.as_array() else {
            return Err(ai_error("INVALID_INPUT", "jobs: expected array of strings"));
        };
        let mut out = Vec::<String>::new();
        for item in arr {
            let Some(s) = item.as_str() else {
                return Err(ai_error("INVALID_INPUT", "jobs: expected array of strings"));
            };
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if !out.iter().any(|v| v == s) {
                out.push(s.to_string());
            }
        }
        if out.is_empty() {
            return Err(ai_error("INVALID_INPUT", "jobs: must not be empty"));
        }
        return Ok(Some(out));
    }

    Ok(None)
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_macro_respond_inbox(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "job",
                "jobs",
                "message",
                "refs",
                "dry_run",
                "limit",
            ],
            "jobs.macro.respond.inbox",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let message = match require_non_empty_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let select_limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(25).clamp(1, 200),
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let explicit_jobs = match normalize_jobs_list_optional(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let selection_mode = if explicit_jobs.is_some() {
            "explicit"
        } else {
            "auto_needs_manager"
        };
        if explicit_jobs.is_some() && args_obj.contains_key("limit") {
            warnings.push(warning(
                "ARG_IGNORED",
                "limit ignored when job/jobs are explicitly provided",
                "Remove limit or omit job/jobs to auto-select inbox targets.",
            ));
        }
        let jobs = if let Some(v) = explicit_jobs {
            v
        } else {
            let radar = match self.store.jobs_radar(
                &workspace,
                bm_storage::JobsRadarRequest {
                    status: None,
                    task_id: None,
                    anchor_id: None,
                    limit: select_limit,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let mut out = Vec::<String>::new();
            for row in radar.rows {
                let needs_manager = row.last_question_seq.unwrap_or(0)
                    > row.last_manager_seq.unwrap_or(0)
                    && (row.job.status == "RUNNING" || row.job.status == "QUEUED");
                if needs_manager && !out.iter().any(|id| id == &row.job.id) {
                    out.push(row.job.id);
                }
            }
            if out.is_empty() {
                warnings.push(warning(
                    "NO_MATCHING_JOBS",
                    "No inbox jobs require manager response in the current scope.",
                    "Wait for a new question/proof-gate or pass job/jobs explicitly.",
                ));
            }
            out
        };

        // Proof-first DX: salvage stable refs from free-form message (adds LINK/CMD/FILE where obvious).
        if let Some(first_job) = jobs.first() {
            refs = crate::salvage_job_completion_refs(&message, first_job, &refs);
        }

        let mut posted = Vec::<Value>::new();
        if !dry_run {
            for job_id in &jobs {
                match self.store.job_message(
                    &workspace,
                    bm_storage::JobMessageRequest {
                        id: job_id.clone(),
                        message: message.clone(),
                        refs: refs.clone(),
                    },
                ) {
                    Ok(res) => posted.push(json!({
                        "job": job_row_to_json(res.job),
                        "event": job_event_to_json(res.event)
                    })),
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
                    Err(StoreError::JobNotMessageable { job_id, status }) => {
                        return ai_error_with(
                            "CONFLICT",
                            &format!("job is not messageable (job_id={job_id}, status={status})"),
                            Some(
                                "Open the job to inspect its status; message is allowed only for QUEUED/RUNNING jobs.",
                            ),
                            Vec::new(),
                        );
                    }
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            }
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "dry_run": dry_run,
            "selection": {
                "mode": selection_mode,
                "limit": select_limit
            },
            "jobs": jobs,
            "posted": posted,
            "count": posted.len()
        });
        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_respond_inbox", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_respond_inbox",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    pub(crate) fn tool_tasks_jobs_macro_dispatch_slice(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "title",
                "prompt",
                "task",
                "anchor",
                "priority",
                "executor",
                "executor_profile",
                "executor_model",
                "policy",
                "expected_artifacts",
                "meta",
                "dry_run",
            ],
            "jobs.macro.dispatch.slice",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let title = match require_non_empty_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let prompt = match require_non_empty_string(args_obj, "prompt") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task_id = match optional_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let anchor_id = match optional_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let priority = match optional_string(args_obj, "priority") {
            Ok(v) => v.unwrap_or_else(|| "MEDIUM".to_string()),
            Err(resp) => return resp,
        };
        let executor = match optional_string(args_obj, "executor") {
            Ok(v) => v.unwrap_or_else(|| "auto".to_string()),
            Err(resp) => return resp,
        };
        let executor_profile = match optional_string(args_obj, "executor_profile") {
            Ok(v) => v.unwrap_or_else(|| "xhigh".to_string()),
            Err(resp) => return resp,
        };
        let executor_model = match optional_string(args_obj, "executor_model") {
            Ok(v) => v.unwrap_or_else(|| "gpt-5.3-codex".to_string()),
            Err(resp) => return resp,
        };
        let expected_artifacts = match optional_string_array(args_obj, "expected_artifacts") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let policy = args_obj.get("policy").cloned().unwrap_or(Value::Null);
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        // Mirror jobs.create routing meta, but mark as macro-dispatched for auditability.
        let mut meta_obj = args_obj
            .get("meta")
            .cloned()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        meta_obj.insert("executor".to_string(), Value::String(executor.clone()));
        meta_obj.insert(
            "executor_profile".to_string(),
            Value::String(executor_profile.clone()),
        );
        meta_obj.insert(
            "executor_model".to_string(),
            Value::String(executor_model.clone()),
        );
        meta_obj.insert(
            "dispatched_by".to_string(),
            Value::String("jobs.macro.dispatch.slice".to_string()),
        );
        if !expected_artifacts.is_empty() {
            meta_obj.insert(
                "expected_artifacts".to_string(),
                Value::Array(
                    expected_artifacts
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>(),
                ),
            );
        }
        if !policy.is_null() {
            meta_obj.insert("policy".to_string(), policy.clone());
        }

        if executor == "auto"
            && let Some(selection) = executor_routing::auto_route_executor(
                self,
                &workspace,
                &executor_profile,
                &expected_artifacts,
                &policy,
            )
        {
            meta_obj.insert("routing".to_string(), selection);
        }
        let meta_json = serde_json::to_string(&Value::Object(meta_obj)).ok();

        let created = if dry_run {
            None
        } else {
            match self.store.job_create(
                &workspace,
                bm_storage::JobCreateRequest {
                    title,
                    prompt,
                    kind: "codex_cli".to_string(),
                    priority,
                    task_id,
                    anchor_id,
                    meta_json,
                },
            ) {
                Ok(v) => Some(v),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "dry_run": dry_run,
            "job": Value::Null,
            "event": Value::Null
        });
        if let Some(created) = created
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("job".to_string(), job_row_to_json(created.job));
            obj.insert(
                "event".to_string(),
                job_event_to_json(created.created_event),
            );
        }

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_dispatch_slice", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_dispatch_slice",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    pub(crate) fn tool_tasks_jobs_macro_enforce_proof(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "job",
                "jobs",
                "message",
                "refs",
                "dry_run",
                "limit",
            ],
            "jobs.macro.enforce.proof",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let message = match optional_string(args_obj, "message") {
            Ok(v) => v.unwrap_or_else(|| "proof ack".to_string()),
            Err(resp) => return resp,
        };
        let refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        if !ensure_has_proof_ref(&refs) {
            return ai_error(
                "INVALID_INPUT",
                "refs must include at least one proof receipt (LINK:/CMD:/FILE:)",
            );
        }
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let select_limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(25).clamp(1, 200),
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        let explicit_jobs = match normalize_jobs_list_optional(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let selection_mode = if explicit_jobs.is_some() {
            "explicit"
        } else {
            "auto_needs_proof"
        };
        if explicit_jobs.is_some() && args_obj.contains_key("limit") {
            warnings.push(warning(
                "ARG_IGNORED",
                "limit ignored when job/jobs are explicitly provided",
                "Remove limit or omit job/jobs to auto-select proof-gate targets.",
            ));
        }
        let jobs = if let Some(v) = explicit_jobs {
            v
        } else {
            let radar = match self.store.jobs_radar(
                &workspace,
                bm_storage::JobsRadarRequest {
                    status: None,
                    task_id: None,
                    anchor_id: None,
                    limit: select_limit,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let mut out = Vec::<String>::new();
            for row in radar.rows {
                let needs_proof = row.last_proof_gate_seq.unwrap_or(0)
                    > row
                        .last_checkpoint_seq
                        .unwrap_or(0)
                        .max(row.last_manager_proof_seq.unwrap_or(0))
                    && row.job.status == "RUNNING";
                if needs_proof && !out.iter().any(|id| id == &row.job.id) {
                    out.push(row.job.id);
                }
            }
            if out.is_empty() {
                warnings.push(warning(
                    "NO_MATCHING_JOBS",
                    "No jobs currently require proof acknowledgment in the current scope.",
                    "Wait for a proof_gate event or pass job/jobs explicitly.",
                ));
            }
            out
        };

        let mut posted = Vec::<Value>::new();
        if !dry_run {
            for job_id in &jobs {
                match self.store.job_message(
                    &workspace,
                    bm_storage::JobMessageRequest {
                        id: job_id.clone(),
                        message: message.clone(),
                        refs: refs.clone(),
                    },
                ) {
                    Ok(res) => posted.push(json!({
                        "job": job_row_to_json(res.job),
                        "event": job_event_to_json(res.event)
                    })),
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
                    Err(StoreError::JobNotMessageable { job_id, status }) => {
                        return ai_error_with(
                            "CONFLICT",
                            &format!("job is not messageable (job_id={job_id}, status={status})"),
                            Some(
                                "Open the job to inspect its status; message is allowed only for QUEUED/RUNNING jobs.",
                            ),
                            Vec::new(),
                        );
                    }
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            }
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "dry_run": dry_run,
            "selection": {
                "mode": selection_mode,
                "limit": select_limit
            },
            "jobs": jobs,
            "posted": posted,
            "count": posted.len()
        });
        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_enforce_proof", result)
        } else {
            ai_ok_with_warnings(
                "tasks_jobs_macro_enforce_proof",
                result,
                warnings,
                Vec::new(),
            )
        }
    }

    pub(crate) fn tool_tasks_jobs_macro_sync_team(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if !self.jobs_mesh_v1_enabled {
            return ai_error_with(
                "NOT_ENABLED",
                "jobs mesh v1 is disabled",
                Some("Enable via BRANCHMIND_JOBS_MESH_V1=1 (or --jobs-mesh-v1)."),
                Vec::new(),
            );
        }

        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "plan_delta",
                "idempotency_key",
                "from_agent_id",
            ],
            "jobs.macro.sync.team",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match require_non_empty_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let idempotency_key = match require_non_empty_string(args_obj, "idempotency_key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_agent_id = match optional_agent_id(args_obj, "from_agent_id") {
            Ok(v) => v
                .or_else(|| self.default_agent_id.clone())
                .unwrap_or_else(|| "manager".to_string()),
            Err(resp) => return resp,
        };
        let plan_delta = args_obj.get("plan_delta").cloned().unwrap_or(Value::Null);
        if plan_delta.is_null() {
            return ai_error("INVALID_INPUT", "plan_delta is required");
        }

        let payload_json = serde_json::to_string(&json!({ "plan_delta": plan_delta })).ok();
        let published = match self.store.job_bus_publish(
            &workspace,
            bm_storage::JobBusPublishRequest {
                idempotency_key,
                thread_id: format!("task/{task_id}"),
                from_agent_id,
                from_job_id: None,
                to_agent_id: None,
                kind: "plan_delta".to_string(),
                summary: "plan delta".to_string(),
                refs: Vec::new(),
                payload_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let m = published.message;
        let result = json!({
            "workspace": workspace.as_str(),
            "deduped": published.deduped,
            "message": {
                "seq": m.seq,
                "ts_ms": m.ts_ms,
                "thread_id": m.thread_id,
                "from_agent_id": m.from_agent_id,
                "kind": m.kind,
                "summary": m.summary,
                "refs": m.refs,
                "payload_json": m.payload_json,
                "idempotency_key": m.idempotency_key
            }
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_macro_sync_team", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_macro_sync_team", result, warnings, Vec::new())
        }
    }
}
