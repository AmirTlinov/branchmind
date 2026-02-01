#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

fn job_row_to_json(job: bm_storage::JobRow) -> Value {
    json!({
        "job_id": job.id,
        "revision": job.revision,
        "status": job.status,
        "title": job.title,
        "kind": job.kind,
        "priority": job.priority,
        "task": job.task_id,
        "anchor": job.anchor_id,
        "runner": job.runner,
        "claim_expires_at_ms": job.claim_expires_at_ms,
        "summary": job.summary,
        "created_at_ms": job.created_at_ms,
        "updated_at_ms": job.updated_at_ms,
        "completed_at_ms": job.completed_at_ms
    })
}

fn job_event_to_json(event: bm_storage::JobEventRow) -> Value {
    let job_id = event.job_id;
    let seq = event.seq;
    let job_ref = format!("{job_id}@{seq}");

    let mut out = json!({
        "seq": seq,
        "job_id": job_id,
        "ref": job_ref,
        "ts_ms": event.ts_ms,
        "kind": event.kind,
        "message": event.message,
        "percent": event.percent,
        "refs": event.refs
    });

    if let Some(meta_json) = event.meta_json.as_deref()
        && let Ok(meta) = serde_json::from_str::<Value>(meta_json)
        && let Some(obj) = out.as_object_mut()
    {
        obj.insert("meta".to_string(), meta);
    }

    out
}

fn runner_lease_to_json(row: bm_storage::RunnerLeaseRow) -> Value {
    json!({
        "runner_id": row.runner_id,
        "status": row.status,
        "active_job_id": row.active_job_id,
        "lease_expires_at_ms": row.lease_expires_at_ms,
        "created_at_ms": row.created_at_ms,
        "updated_at_ms": row.updated_at_ms
    })
}

fn runner_lease_offline_to_json(row: bm_storage::RunnerLeaseRow) -> Value {
    json!({
        "runner_id": row.runner_id,
        "status": "offline",
        "last_status": row.status,
        "active_job_id": row.active_job_id,
        "lease_expires_at_ms": row.lease_expires_at_ms,
        "created_at_ms": row.created_at_ms,
        "updated_at_ms": row.updated_at_ms
    })
}

fn runner_status_to_json(s: bm_storage::RunnerStatusSnapshot) -> Value {
    json!({
        "status": s.status,
        "live_count": s.live_count,
        "idle_count": s.idle_count,
        "offline_count": s.offline_count,
        "runner_id": s.runner_id,
        "active_job_id": s.active_job_id,
        "lease_expires_at_ms": s.lease_expires_at_ms
    })
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let title = match require_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let prompt = match require_string(args_obj, "prompt") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let kind = match optional_string(args_obj, "kind") {
            Ok(v) => v.unwrap_or_else(|| "codex_cli".to_string()),
            Err(resp) => return resp,
        };
        let priority = match optional_string(args_obj, "priority") {
            Ok(v) => v.unwrap_or_else(|| "MEDIUM".to_string()),
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
        let executor = match optional_string(args_obj, "executor") {
            Ok(v) => v.unwrap_or_else(|| "auto".to_string()),
            Err(resp) => return resp,
        };
        let executor_profile = match optional_string(args_obj, "executor_profile") {
            Ok(v) => v.unwrap_or_else(|| "fast".to_string()),
            Err(resp) => return resp,
        };
        let expected_artifacts = match optional_string_array(args_obj, "expected_artifacts") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let policy = args_obj.get("policy").cloned().unwrap_or(Value::Null);

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
        if !expected_artifacts.is_empty() {
            meta_obj.insert(
                "expected_artifacts".to_string(),
                Value::Array(
                    expected_artifacts
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if !policy.is_null() {
            meta_obj.insert("policy".to_string(), policy.clone());
        }

        if executor == "auto"
            && let Some(selection) = auto_route_executor(
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

        let created = match self.store.job_create(
            &workspace,
            bm_storage::JobCreateRequest {
                title,
                prompt,
                kind,
                priority,
                task_id,
                anchor_id,
                meta_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(created.job),
            "event": job_event_to_json(created.created_event)
        });
        ai_ok("tasks_jobs_create", result)
    }

    pub(crate) fn tool_tasks_jobs_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
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
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(25).clamp(1, 200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let list = match self.store.jobs_list(
            &workspace,
            bm_storage::JobsListRequest {
                status,
                task_id,
                anchor_id,
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let jobs_json = list
            .jobs
            .into_iter()
            .map(job_row_to_json)
            .collect::<Vec<_>>();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "jobs": jobs_json,
            "count": jobs_json.len(),
            "has_more": list.has_more,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "jobs", limit);

            if let Some(obj) = result.as_object_mut()
                && let Some(jobs) = obj.get("jobs").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(jobs.len() as u64)),
                );
                let has_more = obj
                    .get("has_more")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if budget_truncated && !has_more {
                    obj.insert("has_more".to_string(), Value::Bool(true));
                }
            }

            set_truncated_flag(&mut result, budget_truncated);
            let _used = attach_budget(&mut result, limit, budget_truncated);

            let warnings = budget_warnings(budget_truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("tasks_jobs_list", result)
            } else {
                ai_ok_with_warnings("tasks_jobs_list", result, warnings, Vec::new())
            }
        } else {
            ai_ok("tasks_jobs_list", result)
        }
    }

    pub(crate) fn tool_tasks_jobs_radar(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        // Optional inbox shortcut: allow a manager reply in the same call to keep daily UX cheap.
        // This is a convenience wrapper over `tasks_jobs_message`.
        let reply_job = match optional_string(args_obj, "reply_job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let reply_message = match optional_string(args_obj, "reply_message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut reply_refs = match optional_string_array(args_obj, "reply_refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        let replied = if let Some(reply_job) = reply_job
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let Some(reply_message) = reply_message
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return ai_error(
                    "INVALID_INPUT",
                    "reply_message must be provided when reply_job is set",
                );
            };

            // Proof-first DX: augment refs with any receipts/refs found in free-form message text.
            // We never remove explicit refs; we only add stable refs that reduce needless loops.
            if !reply_message.trim().is_empty() {
                reply_refs =
                    crate::salvage_job_completion_refs(reply_message, reply_job, &reply_refs);
            }

            match self.store.job_message(
                &workspace,
                bm_storage::JobMessageRequest {
                    id: reply_job.to_string(),
                    message: reply_message.to_string(),
                    refs: reply_refs.clone(),
                },
            ) {
                Ok(posted) => Some(json!({
                    "job": job_row_to_json(posted.job),
                    "event": job_event_to_json(posted.event)
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
        } else {
            None
        };

        let status = match optional_string(args_obj, "status") {
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
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(25).clamp(1, 200),
            Err(resp) => return resp,
        };
        let runners_limit = match optional_usize(args_obj, "runners_limit") {
            Ok(v) => v.unwrap_or(10).clamp(1, 50),
            Err(resp) => return resp,
        };
        let offline_limit = match optional_usize(args_obj, "offline_limit") {
            Ok(v) => v.unwrap_or(3).min(50),
            Err(resp) => return resp,
        };
        let runners_status = match optional_string(args_obj, "runners_status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let _stale_after_s = match optional_usize(args_obj, "stale_after_s") {
            Ok(v) => v.unwrap_or(600).clamp(60, 86_400),
            Err(resp) => return resp,
        } as i64;
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let radar = match self.store.jobs_radar(
            &workspace,
            bm_storage::JobsRadarRequest {
                status,
                task_id,
                anchor_id,
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let now_ms = crate::support::now_ms_i64();
        let runner_status = match self.store.runner_status_snapshot(&workspace, now_ms) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let runner_is_offline = runner_status.status == "offline";
        let runner_leases = match self.store.runner_leases_list_active(
            &workspace,
            now_ms,
            bm_storage::RunnerLeasesListRequest {
                status: runners_status,
                limit: runners_limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let bm_storage::RunnerLeasesListResult {
            runners: runner_rows,
            has_more: runner_leases_has_more,
        } = runner_leases;

        let runner_offline = if offline_limit > 0 && runner_status.offline_count > 0 {
            match self.store.runner_leases_list_offline_recent(
                &workspace,
                now_ms,
                bm_storage::RunnerLeasesListOfflineRequest {
                    limit: offline_limit,
                },
            ) {
                Ok(v) => Some(v),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            None
        };

        // Runner liveness map for job lines (explicit, no heuristics).
        //
        // We must avoid the "runner=unknown" ambiguity when the runner lease list is truncated:
        // if a RUNNING job references a runner_id, we should be able to show live/idle/offline
        // deterministically by consulting the persisted lease (runner_lease_get), not by guessing
        // from partial lists.
        let mut runner_state_cache = HashMap::<String, String>::new();
        for lease in &runner_rows {
            runner_state_cache.insert(lease.runner_id.clone(), lease.status.clone());
        }
        let mut jobs_json = Vec::<Value>::new();
        let mut has_queued = false;
        for row in radar.rows {
            let bm_storage::JobRadarRow {
                job,
                last_event: last,
                last_question_seq,
                last_manager_seq,
                last_manager_proof_seq,
                last_error_seq,
                last_proof_gate_seq,
                last_checkpoint_seq,
            } = row;

            let needs_manager = last_question_seq.unwrap_or(0) > last_manager_seq.unwrap_or(0)
                && (job.status == "RUNNING" || job.status == "QUEUED");
            let has_error = last_error_seq.unwrap_or(0) > last_checkpoint_seq.unwrap_or(0)
                && job.status == "RUNNING";
            let needs_proof = last_proof_gate_seq.unwrap_or(0)
                > last_checkpoint_seq
                    .unwrap_or(0)
                    .max(last_manager_proof_seq.unwrap_or(0))
                && job.status == "RUNNING";
            let stale = job.status == "RUNNING"
                && job.claim_expires_at_ms.map(|v| v <= now_ms).unwrap_or(true);
            if job.status == "QUEUED" {
                has_queued = true;
            }

            // Compute runner_state for RUNNING jobs (live/idle/offline), independent of whether
            // the runner leases list is truncated.
            let runner_state = if job.status == "RUNNING" {
                job.runner.as_deref().and_then(|rid| {
                    let rid = rid.trim();
                    if rid.is_empty() {
                        return None;
                    }
                    if let Some(state) = runner_state_cache.get(rid) {
                        return Some(state.clone());
                    }
                    let fetched = match self.store.runner_lease_get(
                        &workspace,
                        bm_storage::RunnerLeaseGetRequest {
                            runner_id: rid.to_string(),
                        },
                    ) {
                        Ok(Some(res)) => {
                            if res.lease.lease_expires_at_ms > now_ms {
                                res.lease.status
                            } else {
                                "offline".to_string()
                            }
                        }
                        Ok(None) => "offline".to_string(),
                        Err(_) => "offline".to_string(),
                    };
                    runner_state_cache.insert(rid.to_string(), fetched.clone());
                    Some(fetched)
                })
            } else {
                None
            };

            let mut job_json = job_row_to_json(job);
            if let Some(obj) = job_json.as_object_mut() {
                obj.insert(
                    "last".to_string(),
                    last.map(job_event_to_json).unwrap_or(Value::Null),
                );
                obj.insert(
                    "attention".to_string(),
                    json!({
                        "needs_manager": needs_manager,
                        "needs_proof": needs_proof,
                        "has_error": has_error,
                        "stale": stale
                    }),
                );
                if let Some(state) = runner_state {
                    obj.insert("runner_state".to_string(), Value::String(state));
                }
            }
            jobs_json.push(job_json);
        }

        // Multi-runner conflict diagnostics (bounded, no heuristics).
        // Goal: managers can see “who is live and where it’s stuck” without hunting or guessing.
        let mut runner_issues = Vec::<Value>::new();
        let mut issues_has_more = false;
        let max_issues = 20usize;

        let runners_complete = !runner_leases_has_more;
        let mut active_runner_ids = HashSet::<String>::new();
        for r in &runner_rows {
            active_runner_ids.insert(r.runner_id.clone());
        }

        let mut job_cache = HashMap::<String, Option<bm_storage::JobRow>>::new();
        let mut live_job_to_runners = HashMap::<String, Vec<String>>::new();

        for lease in &runner_rows {
            let rid = lease.runner_id.as_str();
            let status = lease.status.as_str();
            let active_job = lease.active_job_id.as_deref();

            match (status, active_job) {
                ("live", None) => {
                    if runner_issues.len() < max_issues {
                        runner_issues.push(json!({
                            "severity": "error",
                            "kind": "live_missing_active_job",
                            "runner_id": rid,
                            "message": "runner is live but active_job_id is missing"
                        }));
                    } else {
                        issues_has_more = true;
                    }
                    continue;
                }
                ("idle", Some(job_id)) => {
                    if runner_issues.len() < max_issues {
                        runner_issues.push(json!({
                            "severity": "error",
                            "kind": "idle_has_active_job",
                            "runner_id": rid,
                            "job_id": job_id,
                            "message": "runner is idle but active_job_id is set"
                        }));
                    } else {
                        issues_has_more = true;
                    }
                    // Keep going: still useful to validate the job link.
                }
                _ => {}
            }

            if status == "live"
                && let Some(job_id) = active_job
            {
                live_job_to_runners
                    .entry(job_id.to_string())
                    .or_default()
                    .push(rid.to_string());

                let job_row = if let Some(cached) = job_cache.get(job_id) {
                    cached.clone()
                } else {
                    let fetched = self
                        .store
                        .job_get(
                            &workspace,
                            bm_storage::JobGetRequest {
                                id: job_id.to_string(),
                            },
                        )
                        .unwrap_or_default();
                    job_cache.insert(job_id.to_string(), fetched.clone());
                    fetched
                };

                match job_row {
                    None => {
                        if runner_issues.len() < max_issues {
                            runner_issues.push(json!({
                                "severity": "error",
                                "kind": "active_job_unknown",
                                "runner_id": rid,
                                "job_id": job_id,
                                "message": "runner references unknown job"
                            }));
                        } else {
                            issues_has_more = true;
                        }
                    }
                    Some(job_row) => {
                        if job_row.status != "RUNNING" {
                            if runner_issues.len() < max_issues {
                                runner_issues.push(json!({
                                    "severity": "error",
                                    "kind": "active_job_not_running",
                                    "runner_id": rid,
                                    "job_id": job_id,
                                    "job_status": job_row.status,
                                    "message": "runner is live on a job that is not RUNNING"
                                }));
                            } else {
                                issues_has_more = true;
                            }
                        } else if job_row.runner.as_deref() != Some(rid) {
                            if runner_issues.len() < max_issues {
                                runner_issues.push(json!({
                                        "severity": "error",
                                        "kind": "job_runner_mismatch",
                                        "runner_id": rid,
                                        "job_id": job_id,
                                        "job_runner_id": job_row.runner,
                                        "message": "runner is live on a job that is claimed by a different runner (likely reclaimed)"
                                    }));
                            } else {
                                issues_has_more = true;
                            }
                        } else if job_row
                            .claim_expires_at_ms
                            .map(|v| v <= now_ms)
                            .unwrap_or(true)
                        {
                            if runner_issues.len() < max_issues {
                                runner_issues.push(json!({
                                        "severity": "stale",
                                        "kind": "job_claim_expired",
                                        "runner_id": rid,
                                        "job_id": job_id,
                                        "claim_expires_at_ms": job_row.claim_expires_at_ms,
                                        "message": "job claim lease expired while runner is live (runner likely stuck)"
                                    }));
                            } else {
                                issues_has_more = true;
                            }
                        }
                    }
                }
            }
        }

        // Duplicate active_job_id among live runners is a hard conflict.
        for (job_id, runners) in live_job_to_runners.iter() {
            if runners.len() <= 1 {
                continue;
            }
            if runner_issues.len() < max_issues {
                runner_issues.push(json!({
                    "severity": "error",
                    "kind": "duplicate_active_job",
                    "job_id": job_id,
                    "runners": runners,
                    "message": "multiple live runners report the same active_job_id"
                }));
            } else {
                issues_has_more = true;
            }
        }

        // If we have the complete active runner lease set, detect RUNNING jobs whose runner has no
        // active lease (stuck / runner offline / old runner version).
        if runners_complete {
            for job in &jobs_json {
                let Some(obj) = job.as_object() else {
                    continue;
                };
                let status = obj.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                if status != "RUNNING" {
                    continue;
                }
                let runner_id = obj
                    .get("runner")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty());
                let Some(runner_id) = runner_id else {
                    continue;
                };
                if active_runner_ids.contains(runner_id) {
                    continue;
                }
                let job_id = obj.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
                if runner_issues.len() < max_issues {
                    runner_issues.push(json!({
                        "severity": "warn",
                        "kind": "job_runner_offline",
                        "job_id": job_id,
                        "runner_id": runner_id,
                        "message": "job is RUNNING but runner has no active lease"
                    }));
                } else {
                    issues_has_more = true;
                }
            }
        }

        let runner_leases_json = runner_rows
            .into_iter()
            .map(runner_lease_to_json)
            .collect::<Vec<_>>();
        let runner_offline_json = runner_offline
            .as_ref()
            .map(|v| {
                v.runners
                    .iter()
                    .cloned()
                    .map(runner_lease_offline_to_json)
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        let runner_offline_has_more = runner_offline.as_ref().is_some_and(|v| v.has_more);

        let runner_autostart_active = self.maybe_autostart_runner(
            &workspace,
            now_ms,
            if has_queued { 1 } else { 0 },
            runner_is_offline,
        );

        let mut result = json!({
            "workspace": workspace.as_str(),
            "runner_status": runner_status_to_json(runner_status),
            "runner_leases": {
                "runners": runner_leases_json,
                "count": runner_leases_json.len(),
                "has_more": runner_leases_has_more
            },
            "runner_leases_offline": {
                "runners": runner_offline_json,
                "count": runner_offline_json.len(),
                "has_more": runner_offline_has_more
            },
            "runner_diagnostics": {
                "issues": runner_issues,
                "count": runner_issues.len(),
                "has_more": issues_has_more
            },
            "jobs": jobs_json,
            "count": jobs_json.len(),
            "has_more": radar.has_more,
            "truncated": false
        });

        // UX: when there are queued jobs, provide a copy/paste runner start hint.
        // This avoids the common "jobs stay QUEUED => runner must be offline / misconfigured" hunt.
        if has_queued && runner_is_offline && !runner_autostart_active {
            let storage_dir = self.store.storage_dir();
            let storage_dir =
                std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
            let mcp_bin =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("bm_mcp"));
            let runner_bin = mcp_bin
                .parent()
                .map(|dir| dir.join("bm_runner"))
                .filter(|p| p.exists())
                .unwrap_or_else(|| std::path::PathBuf::from("bm_runner"));

            let cmd = format!(
                "\"{}\" --storage-dir \"{}\" --workspace \"{}\" --mcp-bin \"{}\"",
                runner_bin.to_string_lossy(),
                storage_dir.to_string_lossy(),
                workspace.as_str(),
                mcp_bin.to_string_lossy()
            );
            if let Some(obj) = result.as_object_mut() {
                obj.insert(
                    "runner_bootstrap".to_string(),
                    json!({
                        "cmd": cmd,
                        "runner_bin": runner_bin.to_string_lossy(),
                        "mcp_bin": mcp_bin.to_string_lossy(),
                        "storage_dir": storage_dir.to_string_lossy()
                    }),
                );
            }
        }
        if let Some(reply) = replied
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("reply".to_string(), reply);
        }

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, jobs_budget_truncated) =
                enforce_graph_list_budget(&mut result, "jobs", limit);
            let mut truncated = jobs_budget_truncated;

            // If we're still over budget, trim the runner leases list next.
            if let Some(obj) = result.as_object_mut()
                && let Some(leases) = obj.get_mut("runner_leases")
            {
                let (_used, leases_truncated) = enforce_graph_list_budget(leases, "runners", limit);
                truncated = truncated || leases_truncated;

                if let Some(leases_obj) = leases.as_object_mut()
                    && let Some(runners) = leases_obj.get("runners").and_then(|v| v.as_array())
                {
                    leases_obj.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(runners.len() as u64)),
                    );
                    let has_more = leases_obj
                        .get("has_more")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if leases_truncated && !has_more {
                        leases_obj.insert("has_more".to_string(), Value::Bool(true));
                    }
                }
            }

            // Then trim offline runner leases (recent offline) if needed.
            if let Some(obj) = result.as_object_mut()
                && let Some(leases) = obj.get_mut("runner_leases_offline")
            {
                let (_used, leases_truncated) = enforce_graph_list_budget(leases, "runners", limit);
                truncated = truncated || leases_truncated;

                if let Some(leases_obj) = leases.as_object_mut()
                    && let Some(runners) = leases_obj.get("runners").and_then(|v| v.as_array())
                {
                    leases_obj.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(runners.len() as u64)),
                    );
                    let has_more = leases_obj
                        .get("has_more")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if leases_truncated && !has_more {
                        leases_obj.insert("has_more".to_string(), Value::Bool(true));
                    }
                }
            }

            // Then trim runner diagnostics (issues) if needed.
            if let Some(obj) = result.as_object_mut()
                && let Some(diag) = obj.get_mut("runner_diagnostics")
            {
                let (_used, diag_truncated) = enforce_graph_list_budget(diag, "issues", limit);
                truncated = truncated || diag_truncated;

                if let Some(diag_obj) = diag.as_object_mut()
                    && let Some(issues) = diag_obj.get("issues").and_then(|v| v.as_array())
                {
                    diag_obj.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(issues.len() as u64)),
                    );
                    let has_more = diag_obj
                        .get("has_more")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if diag_truncated && !has_more {
                        diag_obj.insert("has_more".to_string(), Value::Bool(true));
                    }
                }
            }

            // As a last resort, drop the runner bootstrap hint (long command) to stay within a
            // strict max_chars budget.
            if json_len_chars(&result) > limit {
                if let Some(obj) = result.as_object_mut()
                    && obj.remove("runner_bootstrap").is_some()
                {
                    truncated = true;
                }
                let (_used2, jobs_trunc2) = enforce_graph_list_budget(&mut result, "jobs", limit);
                truncated = truncated || jobs_trunc2;
                if let Some(obj) = result.as_object_mut()
                    && let Some(leases) = obj.get_mut("runner_leases")
                {
                    let (_used3, leases_trunc3) =
                        enforce_graph_list_budget(leases, "runners", limit);
                    truncated = truncated || leases_trunc3;
                }
                if let Some(obj) = result.as_object_mut()
                    && let Some(leases) = obj.get_mut("runner_leases_offline")
                {
                    let (_used3, leases_trunc3) =
                        enforce_graph_list_budget(leases, "runners", limit);
                    truncated = truncated || leases_trunc3;
                }
                if let Some(obj) = result.as_object_mut()
                    && let Some(diag) = obj.get_mut("runner_diagnostics")
                {
                    let (_used4, diag_trunc4) = enforce_graph_list_budget(diag, "issues", limit);
                    truncated = truncated || diag_trunc4;
                }
            }

            if let Some(obj) = result.as_object_mut()
                && let Some(jobs) = obj.get("jobs").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(jobs.len() as u64)),
                );
                let has_more = obj
                    .get("has_more")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if jobs_budget_truncated && !has_more {
                    obj.insert("has_more".to_string(), Value::Bool(true));
                }
            }

            set_truncated_flag(&mut result, truncated);
            let _used = attach_budget(&mut result, limit, truncated);

            let warnings = budget_warnings(truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("tasks_jobs_radar", result)
            } else {
                ai_ok_with_warnings("tasks_jobs_radar", result, warnings, Vec::new())
            }
        } else {
            ai_ok("tasks_jobs_radar", result)
        }
    }

    /// Runner liveness lease (explicit, no-heuristics).
    ///
    /// Used by external runners to keep a deterministic "runner live/idle/offline" status in
    /// manager inboxes (jobs_radar) without inferring from job events.
    pub(crate) fn tool_tasks_runner_heartbeat(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let runner_id = match require_string(args_obj, "runner_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = match require_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let active_job_id = match optional_string(args_obj, "active_job_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let lease_ttl_ms = match optional_usize(args_obj, "lease_ttl_ms") {
            Ok(v) => v.unwrap_or(20_000).clamp(1_000, 300_000) as u64,
            Err(resp) => return resp,
        };
        let mut meta_obj = args_obj
            .get("meta")
            .cloned()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if let Some(executors) = args_obj.get("executors").and_then(|v| v.as_array()) {
            meta_obj.insert("executors".to_string(), Value::Array(executors.clone()));
        }
        if let Some(profiles) = args_obj.get("profiles").and_then(|v| v.as_array()) {
            meta_obj.insert("profiles".to_string(), Value::Array(profiles.clone()));
        }
        if let Some(artifacts) = args_obj
            .get("supports_artifacts")
            .and_then(|v| v.as_array())
        {
            meta_obj.insert(
                "supports_artifacts".to_string(),
                Value::Array(artifacts.clone()),
            );
        }
        if let Some(max_parallel) = args_obj.get("max_parallel") {
            meta_obj.insert("max_parallel".to_string(), max_parallel.clone());
        }
        if let Some(sandbox_policy) = args_obj.get("sandbox_policy") {
            meta_obj.insert("sandbox_policy".to_string(), sandbox_policy.clone());
        }
        let meta_json = serde_json::to_string(&Value::Object(meta_obj)).ok();

        let lease = match self.store.runner_lease_upsert(
            &workspace,
            bm_storage::RunnerLeaseUpsertRequest {
                runner_id,
                status,
                active_job_id,
                lease_ttl_ms,
                meta_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "lease": runner_lease_to_json(lease)
        });
        ai_ok("tasks_runner_heartbeat", result)
    }

    pub(crate) fn tool_tasks_jobs_open(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let include_prompt = match optional_bool(args_obj, "include_prompt") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let include_events = match optional_bool(args_obj, "include_events") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let include_meta = match optional_bool(args_obj, "include_meta") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let max_events = match optional_usize(args_obj, "max_events") {
            Ok(v) => v.unwrap_or(10).clamp(0, 200),
            Err(resp) => return resp,
        };
        let before_seq = match optional_i64(args_obj, "before_seq") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let before_seq = match before_seq {
            Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "before_seq must be > 0"),
            Some(v) => Some(v),
            None => None,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: job_id,
                include_prompt,
                include_events,
                include_meta,
                max_events,
                before_seq,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let events = open
            .events
            .into_iter()
            .map(job_event_to_json)
            .collect::<Vec<_>>();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(open.job),
            "prompt": open.prompt,
            "events": events,
            "count": events.len(),
            "has_more_events": open.has_more_events,
            "truncated": false
        });

        if include_meta {
            let meta_value = open
                .meta_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .unwrap_or(Value::Null);
            if let Some(obj) = result.as_object_mut() {
                obj.insert("meta".to_string(), meta_value);
            }
        }

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, events_truncated) = enforce_graph_list_budget(&mut result, "events", limit);

            // If we're still over budget and prompt was included, drop it as a last resort.
            let mut truncated = events_truncated;
            if json_len_chars(&result) > limit {
                if let Some(obj) = result.as_object_mut()
                    && obj.remove("prompt").is_some()
                {
                    truncated = true;
                }
                let (_used2, events_truncated2) =
                    enforce_graph_list_budget(&mut result, "events", limit);
                truncated = truncated || events_truncated2;
            }

            if let Some(obj) = result.as_object_mut()
                && let Some(events) = obj.get("events").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(events.len() as u64)),
                );
                let has_more = obj
                    .get("has_more_events")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if truncated && !has_more {
                    obj.insert("has_more_events".to_string(), Value::Bool(true));
                }
            }

            set_truncated_flag(&mut result, truncated);
            let _used = attach_budget(&mut result, limit, truncated);

            let warnings = budget_warnings(truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("tasks_jobs_open", result)
            } else {
                ai_ok_with_warnings("tasks_jobs_open", result, warnings, Vec::new())
            }
        } else {
            ai_ok("tasks_jobs_open", result)
        }
    }

    pub(crate) fn tool_tasks_jobs_tail(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let after_seq = match optional_i64(args_obj, "after_seq") {
            Ok(v) => v.unwrap_or(0),
            Err(resp) => return resp,
        };
        if after_seq < 0 {
            return ai_error("INVALID_INPUT", "after_seq must be >= 0");
        }
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let tail = match self.store.job_events_tail(
            &workspace,
            bm_storage::JobEventsTailRequest {
                id: job_id.clone(),
                after_seq,
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let events_json = tail
            .events
            .into_iter()
            .map(job_event_to_json)
            .collect::<Vec<_>>();
        let mut result = json!({
            "workspace": workspace.as_str(),
            "job_id": tail.job_id,
            "after_seq": tail.after_seq,
            "next_after_seq": tail.next_after_seq,
            "events": events_json,
            "count": events_json.len(),
            "has_more": tail.has_more,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) = enforce_graph_list_budget(&mut result, "events", limit);
            if let Some(obj) = result.as_object_mut()
                && let Some(events) = obj.get("events").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(events.len() as u64)),
                );
                let has_more = obj
                    .get("has_more")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if budget_truncated && !has_more {
                    obj.insert("has_more".to_string(), Value::Bool(true));
                }
            }

            set_truncated_flag(&mut result, budget_truncated);
            let _used = attach_budget(&mut result, limit, budget_truncated);

            let warnings = budget_warnings(budget_truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("tasks_jobs_tail", result)
            } else {
                ai_ok_with_warnings("tasks_jobs_tail", result, warnings, Vec::new())
            }
        } else {
            ai_ok("tasks_jobs_tail", result)
        }
    }

    pub(crate) fn tool_tasks_jobs_claim(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let runner_id = match optional_string(args_obj, "runner_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        }
        .or_else(|| optional_string(args_obj, "runner").ok().flatten());
        let Some(runner_id) = runner_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        else {
            return ai_error("INVALID_INPUT", "runner_id is required");
        };
        let lease_ttl_ms = match optional_i64(args_obj, "lease_ttl_ms") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let lease_ttl_ms = match lease_ttl_ms {
            Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "lease_ttl_ms must be > 0"),
            Some(v) => v as u64,
            None => 180_000,
        };
        let allow_stale = match optional_bool(args_obj, "allow_stale") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let claimed = match self.store.job_claim(
            &workspace,
            bm_storage::JobClaimRequest {
                id: job_id,
                runner_id,
                lease_ttl_ms,
                allow_stale,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::JobNotClaimable { job_id, status }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job is not claimable (job_id={job_id}, status={status})"),
                    Some("Open the job to see its current status; cancel/requeue if needed."),
                    Vec::new(),
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(claimed.job),
            "event": job_event_to_json(claimed.event)
        });
        ai_ok("tasks_jobs_claim", result)
    }

    pub(crate) fn tool_tasks_jobs_message(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let message = match require_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };

        // Proof-first DX: if refs were forgotten but proof/refs are present in message text,
        // salvage stable references deterministically (reduces needless proof-gate loops).
        if !message.trim().is_empty() {
            refs = crate::salvage_job_completion_refs(&message, &job_id, &refs);
        }

        let posted = match self.store.job_message(
            &workspace,
            bm_storage::JobMessageRequest {
                id: job_id,
                message,
                refs,
            },
        ) {
            Ok(v) => v,
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
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(posted.job),
            "event": job_event_to_json(posted.event)
        });
        ai_ok("tasks_jobs_message", result)
    }

    pub(crate) fn tool_tasks_jobs_report(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let runner_id = match require_string(args_obj, "runner_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let claim_revision = match optional_i64(args_obj, "claim_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(claim_revision) = claim_revision else {
            return ai_error("INVALID_INPUT", "claim_revision is required");
        };
        let lease_ttl_ms = match optional_i64(args_obj, "lease_ttl_ms") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let lease_ttl_ms = match lease_ttl_ms {
            Some(v) if v <= 0 => return ai_error("INVALID_INPUT", "lease_ttl_ms must be > 0"),
            Some(v) => v as u64,
            None => 180_000,
        };
        let kind = match optional_string(args_obj, "kind") {
            Ok(v) => v.unwrap_or_else(|| "progress".to_string()),
            Err(resp) => return resp,
        };
        let message = match require_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let percent = match optional_i64(args_obj, "percent") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        let meta_json = args_obj
            .get("meta")
            .cloned()
            .and_then(|v| serde_json::to_string(&v).ok());

        let report = match self.store.job_report(
            &workspace,
            bm_storage::JobReportRequest {
                id: job_id,
                runner_id,
                claim_revision,
                kind,
                message,
                percent,
                refs,
                meta_json,
                lease_ttl_ms,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::JobClaimMismatch { job_id, .. }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job claim mismatch (job_id={job_id})"),
                    Some(
                        "The job lease was reclaimed or rotated. Re-claim the job to obtain a new claim_revision.",
                    ),
                    Vec::new(),
                );
            }
            Err(StoreError::JobNotRunning { job_id, status }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job is not running (job_id={job_id}, status={status})"),
                    Some("Open the job to see its current status; claim it or complete/cancel it."),
                    Vec::new(),
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(report.job),
            "event": job_event_to_json(report.event)
        });
        ai_ok("tasks_jobs_report", result)
    }

    pub(crate) fn tool_tasks_jobs_complete(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let runner_id = match require_string(args_obj, "runner_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let claim_revision = match optional_i64(args_obj, "claim_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(claim_revision) = claim_revision else {
            return ai_error("INVALID_INPUT", "claim_revision is required");
        };
        let status = match require_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let summary = match optional_string(args_obj, "summary") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        let status_norm = status.trim().to_ascii_uppercase();

        // Proof-first DX: if refs were forgotten but proof is present in summary text,
        // salvage stable references deterministically to avoid needless proof-gate loops.
        if status_norm == "DONE"
            && let Some(s) = summary.as_deref()
            && !s.trim().is_empty()
        {
            refs = crate::salvage_job_completion_refs(s, &job_id, &refs);
        }
        // Keep job thread navigable even when callers omit refs (bounded, deterministic).
        if refs.len() < 32 && !refs.iter().any(|r| r == &job_id) {
            refs.push(job_id.clone());
        }
        let meta_json = args_obj
            .get("meta")
            .cloned()
            .and_then(|v| serde_json::to_string(&v).ok());

        let done = match self.store.job_complete(
            &workspace,
            bm_storage::JobCompleteRequest {
                id: job_id,
                runner_id,
                claim_revision,
                status,
                summary,
                refs,
                meta_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::JobClaimMismatch { job_id, .. }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job claim mismatch (job_id={job_id})"),
                    Some(
                        "The job lease was reclaimed or rotated. Re-claim the job to obtain a new claim_revision.",
                    ),
                    Vec::new(),
                );
            }
            Err(StoreError::JobAlreadyTerminal { job_id, status }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job already terminal (job_id={job_id}, status={status})"),
                    Some("Open the job to inspect prior completion and the referenced artifacts."),
                    Vec::new(),
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(done.job),
            "event": job_event_to_json(done.event)
        });
        ai_ok("tasks_jobs_complete", result)
    }

    pub(crate) fn tool_tasks_jobs_requeue(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let reason = match optional_string(args_obj, "reason") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };
        let meta_json = args_obj
            .get("meta")
            .cloned()
            .and_then(|v| serde_json::to_string(&v).ok());

        let requeued = match self.store.job_requeue(
            &workspace,
            bm_storage::JobRequeueRequest {
                id: job_id,
                reason,
                refs,
                meta_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::JobNotRequeueable { job_id, status }) => {
                return ai_error_with(
                    "CONFLICT",
                    &format!("job is not requeueable (job_id={job_id}, status={status})"),
                    Some(
                        "Open the job to see its current status; cancel/complete it first if needed.",
                    ),
                    Vec::new(),
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "job": job_row_to_json(requeued.job),
            "event": job_event_to_json(requeued.event)
        });
        ai_ok("tasks_jobs_requeue", result)
    }
}

#[derive(Default)]
struct ExecutorPolicy {
    prefer: Vec<String>,
    forbid: HashSet<String>,
    min_profile: Option<String>,
}

fn parse_policy(value: &Value) -> ExecutorPolicy {
    let mut policy = ExecutorPolicy::default();
    let Some(obj) = value.as_object() else {
        return policy;
    };
    if let Some(prefer) = obj.get("prefer").and_then(|v| v.as_array()) {
        policy.prefer = prefer
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(forbid) = obj.get("forbid").and_then(|v| v.as_array()) {
        policy.forbid = forbid
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<HashSet<_>>();
    }
    if let Some(min_profile) = obj.get("min_profile").and_then(|v| v.as_str()) {
        policy.min_profile = Some(min_profile.to_string());
    }
    policy
}

fn profile_rank(profile: &str) -> u8 {
    match profile.to_ascii_lowercase().as_str() {
        "fast" => 0,
        "deep" => 1,
        "audit" => 2,
        _ => 0,
    }
}

fn auto_route_executor(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    executor_profile: &str,
    expected_artifacts: &[String],
    policy_value: &Value,
) -> Option<Value> {
    let policy = parse_policy(policy_value);
    let now_ms = crate::support::now_ms_i64();
    let list = server
        .store
        .runner_leases_list_active(
            workspace,
            now_ms,
            bm_storage::RunnerLeasesListRequest {
                limit: 50,
                status: None,
            },
        )
        .ok()?;

    let mut candidates = Vec::<(u8, u8, String, String)>::new(); // (prefer_rank, availability_rank, runner_id, executor)
    for runner in list.runners {
        let meta = server
            .store
            .runner_lease_get(
                workspace,
                bm_storage::RunnerLeaseGetRequest {
                    runner_id: runner.runner_id.clone(),
                },
            )
            .ok()
            .flatten()
            .and_then(|row| row.meta_json)
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or(Value::Null);
        let executors = meta
            .get("executors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["codex".to_string()]);
        let profiles = meta
            .get("profiles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["fast".to_string(), "deep".to_string(), "audit".to_string()]);
        let supports_artifacts = meta
            .get("supports_artifacts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let availability_rank = if runner.status == "idle" { 0 } else { 1 };
        for executor in executors {
            if policy.forbid.contains(&executor) {
                continue;
            }
            if !profiles.iter().any(|p| p == executor_profile) {
                continue;
            }
            if let Some(min_profile) = &policy.min_profile
                && profile_rank(executor_profile) < profile_rank(min_profile)
            {
                continue;
            }
            if !expected_artifacts.is_empty() && !supports_artifacts.is_empty() {
                let missing = expected_artifacts
                    .iter()
                    .any(|item| !supports_artifacts.iter().any(|v| v == item));
                if missing {
                    continue;
                }
            }
            let prefer_rank = policy
                .prefer
                .iter()
                .position(|v| v == &executor)
                .unwrap_or(usize::MAX) as u8;
            candidates.push((
                prefer_rank,
                availability_rank,
                runner.runner_id.clone(),
                executor,
            ));
        }
    }

    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let (_, _, runner_id, executor) = candidates.first()?.clone();
    Some(json!({
        "selected_executor": executor,
        "selected_runner_id": runner_id,
        "policy": {
            "prefer": policy.prefer,
            "forbid": policy.forbid.iter().cloned().collect::<Vec<_>>(),
            "min_profile": policy.min_profile
        }
    }))
}
