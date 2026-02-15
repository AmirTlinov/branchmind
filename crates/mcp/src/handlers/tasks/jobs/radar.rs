#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::{
    check_unknown_args, job_event_to_json, job_row_to_json, push_warning_if,
    runner_lease_offline_to_json, runner_lease_to_json, runner_status_to_json,
};

impl McpServer {
    pub(crate) fn tool_tasks_jobs_radar(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let mut ux_warnings = Vec::<Value>::new();
        // Reserved for Phase C (jobs.mesh.*). Keep the flag live to avoid drift.
        let _ = self.jobs_mesh_v1_enabled;
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "status",
                "task",
                "anchor",
                "limit",
                "runners_limit",
                "runners_status",
                "offline_limit",
                "include_offline",
                "stall_after_s",
                "stale_after_s",
                "reply_job",
                "reply_message",
                "reply_refs",
                "max_chars",
                "fmt",
            ],
            "jobs.radar",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        push_warning_if(&mut ux_warnings, unknown_warning);
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
        let include_offline = match optional_bool(args_obj, "include_offline") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let offline_limit_raw = match optional_usize(args_obj, "offline_limit") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let offline_limit = offline_limit_raw.unwrap_or(3).min(50);
        if !include_offline && offline_limit_raw.is_some() {
            ux_warnings.push(warning(
                "ARG_IGNORED",
                "offline_limit ignored because include_offline=false",
                "Set include_offline=true or remove offline_limit.",
            ));
        }
        let runners_status = match optional_string(args_obj, "runners_status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let stall_after_s = match optional_usize(args_obj, "stall_after_s") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let stale_after_s = match optional_usize(args_obj, "stale_after_s") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if stall_after_s.is_some() && stale_after_s.is_some() {
            return ai_error(
                "INVALID_INPUT",
                "use only one of stall_after_s or stale_after_s",
            );
        }
        if stale_after_s.is_some() {
            ux_warnings.push(warning(
                "DEPRECATED_ARG",
                "stale_after_s is deprecated; use stall_after_s",
                "Rename stale_after_s -> stall_after_s.",
            ));
        }
        let stall_after_input = stall_after_s.or(stale_after_s).unwrap_or(600);
        let stall_after_s = stall_after_input.clamp(60, 86_400) as i64;
        if stall_after_input != stall_after_s as usize {
            ux_warnings.push(warning(
                "ARG_COERCED",
                &format!("stall_after_s coerced to {}", stall_after_s),
                "Use a value in range [60..86400].",
            ));
        }

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

        let runner_offline =
            if include_offline && offline_limit > 0 && runner_status.offline_count > 0 {
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
        let mut stalled_jobs = Vec::<String>::new();
        let mut running_runner_ids = HashSet::<String>::new();
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
                last_checkpoint_ts_ms,
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
            if job.status == "RUNNING"
                && let Some(rid) = job
                    .runner
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            {
                running_runner_ids.insert(rid.to_string());
            }

            let meaningful_at_ms = last_checkpoint_ts_ms
                .or_else(|| last.as_ref().map(|e| e.ts_ms))
                .unwrap_or(job.updated_at_ms);
            let meaningful_age_ms = now_ms.saturating_sub(meaningful_at_ms);
            let stall_after_ms = stall_after_s.saturating_mul(1000);
            let stalled = job.status == "RUNNING" && !stale && meaningful_age_ms >= stall_after_ms;
            if stalled {
                stalled_jobs.push(job.id.clone());
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
                        "stale": stale,
                        "stalled": stalled
                    }),
                );
                obj.insert(
                    "progress".to_string(),
                    json!({
                        "stall_after_s": stall_after_s,
                        "meaningful_at_ms": meaningful_at_ms,
                        "checkpoint_at_ms": last_checkpoint_ts_ms
                    }),
                );
                if let Some(state) = runner_state {
                    obj.insert("runner_state".to_string(), Value::String(state));
                }
            }
            jobs_json.push(job_json);
        }

        let mut suggestions = Vec::<Value>::new();
        if !stalled_jobs.is_empty() {
            ux_warnings.push(warning(
                "JOB_STALLED",
                &format!(
                    "{} job(s) stalled (no checkpoint/progress for >{}s)",
                    stalled_jobs.len(),
                    stall_after_s
                ),
                "Run jobs.macro.rotate.stalled to cancel+recreate stalled RUNNING jobs (or reply via reply_job/reply_message).",
            ));
            suggestions.push(suggest_call(
                "tasks_jobs_macro_rotate_stalled",
                "Rotate stalled RUNNING jobs (cancel + recreate).",
                "high",
                json!({ "stall_after_s": stall_after_s, "limit": stalled_jobs.len().min(5) }),
            ));
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
                    if !running_runner_ids.contains(rid) {
                        continue;
                    }
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
        if include_offline && let Some(obj) = result.as_object_mut() {
            obj.insert(
                "runner_leases_offline".to_string(),
                json!({
                    "runners": runner_offline_json,
                    "count": runner_offline_json.len(),
                    "has_more": runner_offline_has_more
                }),
            );
        }

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

            let mut truncated_final = truncated;
            set_truncated_flag(&mut result, truncated_final);
            let used = attach_budget(&mut result, limit, truncated_final);
            if used > limit && !truncated_final {
                truncated_final = true;
                set_truncated_flag(&mut result, true);
                let _ = attach_budget(&mut result, limit, true);
                ux_warnings.push(warning(
                    "BUDGET_OVERFLOW",
                    "payload exceeds max_chars after trimming",
                    "Increase max_chars or narrow scope/limit to reduce payload size.",
                ));
            }

            let mut warnings = budget_warnings(truncated_final, false, clamped);
            warnings.extend(ux_warnings);
            if warnings.is_empty() && suggestions.is_empty() {
                ai_ok("tasks_jobs_radar", result)
            } else {
                ai_ok_with_warnings("tasks_jobs_radar", result, warnings, suggestions)
            }
        } else if ux_warnings.is_empty() && suggestions.is_empty() {
            ai_ok("tasks_jobs_radar", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_radar", result, ux_warnings, suggestions)
        }
    }
}
