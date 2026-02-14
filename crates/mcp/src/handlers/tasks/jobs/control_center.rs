#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::{check_unknown_args, job_event_to_json, job_row_to_json, push_warning_if};

fn action_call(cmd: &str, reason: &str, priority: &str, args: Value) -> Value {
    json!({
        "op": "call",
        "cmd": cmd,
        "reason": reason,
        "priority": priority,
        "budget_profile": "portal",
        "args": args
    })
}

fn thread_id_for_task(task_id: &str) -> String {
    format!("task/{}", task_id.trim())
}

fn thread_id_for_job(job_id: &str) -> String {
    format!("job/{}", job_id.trim())
}

fn parse_scope_string(args_obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    let scope = args_obj.get("scope")?.as_object()?;
    scope
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn parse_meta_map(meta_json: Option<&str>) -> serde_json::Map<String, Value> {
    meta_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn pipeline_thread_id(task: &str, slice_id: &str) -> String {
    format!("pipeline/{}/{}", task.trim(), slice_id.trim())
}

#[derive(Default, Clone)]
struct PipelineSliceState {
    task: Option<String>,
    slice_id: String,
    scout_pack_ref: Option<String>,
    builder_batch_ref: Option<String>,
    validator_report_ref: Option<String>,
    plan_ref: Option<String>,
    builder_done: bool,
    builder_revision: Option<i64>,
    validator_any: bool,
    validator_done: bool,
    gate_decision: Option<String>,
    gate_decision_ref: Option<String>,
    apply_done: bool,
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_control_center(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "scope",
                "task",
                "anchor",
                "view",
                "limit",
                "stall_after_s",
                "max_chars",
                "fmt",
            ],
            "jobs.control.center",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let task_id = match optional_string(args_obj, "task") {
            Ok(v) => v.or_else(|| parse_scope_string(args_obj, "task")),
            Err(resp) => return resp,
        };
        let anchor_id = match optional_string(args_obj, "anchor") {
            Ok(v) => v.or_else(|| parse_scope_string(args_obj, "anchor")),
            Err(resp) => return resp,
        };
        let view = match optional_string(args_obj, "view") {
            Ok(v) => v.unwrap_or_else(|| "smart".to_string()),
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 200),
            Err(resp) => return resp,
        };
        let stall_after_input = match optional_usize(args_obj, "stall_after_s") {
            Ok(v) => v.unwrap_or(600),
            Err(resp) => return resp,
        };
        let stall_after_s = stall_after_input.clamp(60, 86_400) as i64;
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        if stall_after_input != stall_after_s as usize {
            warnings.push(warning(
                "ARG_COERCED",
                &format!("stall_after_s coerced to {}", stall_after_s),
                "Use a value in range [60..86400].",
            ));
        }

        let now_ms = crate::support::now_ms_i64();

        // Core: jobs radar rows (attention-first, bounded scan).
        let radar = match self.store.jobs_radar(
            &workspace,
            bm_storage::JobsRadarRequest {
                status: None,
                task_id: task_id.clone(),
                anchor_id: anchor_id.clone(),
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Runner status + leases (execution health).
        let runner_status = match self.store.runner_status_snapshot(&workspace, now_ms) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let runner_leases = match self.store.runner_leases_list_active(
            &workspace,
            now_ms,
            bm_storage::RunnerLeasesListRequest {
                status: None,
                limit: 25,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Canonical job rows (with attention hints).
        let mut jobs_json = Vec::<Value>::new();
        let mut inbox_items = Vec::<Value>::new();
        let mut stalled_jobs = Vec::<String>::new();
        let mut needs_proof_jobs = Vec::<String>::new();
        let mut needs_manager_jobs = Vec::<String>::new();
        let mut open_scout_jobs = 0u64;
        let mut open_builder_jobs = 0u64;
        let mut open_validator_jobs = 0u64;
        let mut stale_scout_pack_count = 0u64;
        let mut pipeline_slices =
            std::collections::BTreeMap::<(String, String), PipelineSliceState>::new();

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

            let meaningful_at_ms = last_checkpoint_ts_ms
                .or_else(|| last.as_ref().map(|e| e.ts_ms))
                .unwrap_or(job.updated_at_ms);
            let meaningful_age_ms = now_ms.saturating_sub(meaningful_at_ms);
            let stall_after_ms = stall_after_s.saturating_mul(1000);
            let stalled = job.status == "RUNNING" && !stale && meaningful_age_ms >= stall_after_ms;

            if stalled {
                stalled_jobs.push(job.id.clone());
            }
            if needs_proof {
                needs_proof_jobs.push(job.id.clone());
            }
            if needs_manager {
                needs_manager_jobs.push(job.id.clone());
            }

            let mut job_json = job_row_to_json(job.clone());
            let meta_open = self.store.job_open(
                &workspace,
                bm_storage::JobOpenRequest {
                    id: job.id.clone(),
                    include_prompt: false,
                    include_events: false,
                    include_meta: true,
                    max_events: 0,
                    before_seq: None,
                },
            );
            let meta_map = match meta_open {
                Ok(open) => parse_meta_map(open.meta_json.as_deref()),
                Err(_) => serde_json::Map::new(),
            };
            let pipeline_role = meta_map
                .get("pipeline_role")
                .and_then(|v| v.as_str())
                .or_else(|| meta_map.get("role").and_then(|v| v.as_str()))
                .map(|v| v.trim().to_ascii_lowercase());
            if let Some(role) = pipeline_role.as_deref() {
                let is_open = !matches!(job.status.as_str(), "DONE" | "FAILED" | "CANCELED");
                match role {
                    "scout" => {
                        if is_open {
                            open_scout_jobs = open_scout_jobs.saturating_add(1);
                        }
                        if is_open && now_ms.saturating_sub(job.updated_at_ms) > 5 * 60 * 1000 {
                            stale_scout_pack_count = stale_scout_pack_count.saturating_add(1);
                        }
                    }
                    "builder" => {
                        if is_open {
                            open_builder_jobs = open_builder_jobs.saturating_add(1);
                        }
                    }
                    "validator" => {
                        if is_open {
                            open_validator_jobs = open_validator_jobs.saturating_add(1);
                        }
                    }
                    _ => {}
                }

                let slice_id = meta_map
                    .get("slice_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty());
                let task_for_slice = meta_map
                    .get("pipeline")
                    .and_then(|v| v.get("task"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .or_else(|| job.task_id.clone());

                if let (Some(task_s), Some(slice_s)) = (task_for_slice.clone(), slice_id.clone()) {
                    let key = (task_s.clone(), slice_s.clone());
                    let entry = pipeline_slices
                        .entry(key)
                        .or_insert_with(|| PipelineSliceState {
                            task: Some(task_s.clone()),
                            slice_id: slice_s.clone(),
                            ..PipelineSliceState::default()
                        });
                    entry.task = Some(task_s);
                    entry.slice_id = slice_s;
                    if let Some(scout_pack_ref) =
                        meta_map.get("scout_pack_ref").and_then(|v| v.as_str())
                    {
                        let ref_s = scout_pack_ref.trim();
                        if !ref_s.is_empty() {
                            entry.scout_pack_ref = Some(ref_s.to_string());
                        }
                    }
                    if let Some(builder_batch_ref) =
                        meta_map.get("builder_batch_ref").and_then(|v| v.as_str())
                    {
                        let ref_s = builder_batch_ref.trim();
                        if !ref_s.is_empty() {
                            entry.builder_batch_ref = Some(ref_s.to_string());
                        }
                    }
                    if let Some(plan_ref) = meta_map.get("plan_ref").and_then(|v| v.as_str()) {
                        let ref_s = plan_ref.trim();
                        if !ref_s.is_empty() {
                            entry.plan_ref = Some(ref_s.to_string());
                        }
                    }
                    if role == "builder" && job.status.eq_ignore_ascii_case("DONE") {
                        entry.builder_done = true;
                        entry.builder_revision = Some(job.revision);
                        if entry.builder_batch_ref.is_none() {
                            entry.builder_batch_ref =
                                Some(format!("artifact://jobs/{}/builder_diff_batch", job.id));
                        }
                    }
                    if role == "validator" {
                        entry.validator_any = true;
                        if let Some(report_ref) = meta_map
                            .get("validator_report_ref")
                            .and_then(|v| v.as_str())
                        {
                            let ref_s = report_ref.trim();
                            if !ref_s.is_empty() {
                                entry.validator_report_ref = Some(ref_s.to_string());
                            }
                        }
                        if job.status.eq_ignore_ascii_case("DONE") {
                            entry.validator_done = true;
                            if entry.validator_report_ref.is_none() {
                                entry.validator_report_ref =
                                    Some(format!("artifact://jobs/{}/validator_report", job.id));
                            }
                        }
                    }
                    if role == "scout" && entry.scout_pack_ref.is_none() {
                        entry.scout_pack_ref =
                            Some(format!("artifact://jobs/{}/scout_context_pack", job.id));
                    }
                }
            }
            if let Some(obj) = job_json.as_object_mut() {
                obj.insert(
                    "last".to_string(),
                    last.clone().map(job_event_to_json).unwrap_or(Value::Null),
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
                if let Some(role) = pipeline_role {
                    obj.insert(
                        "pipeline".to_string(),
                        json!({
                            "role": role,
                            "slice_id": meta_map.get("slice_id").cloned().unwrap_or(Value::Null),
                            "task": meta_map
                                .get("pipeline")
                                .and_then(|v| v.get("task"))
                                .cloned()
                                .or_else(|| job.task_id.as_ref().map(|v| Value::String(v.clone())))
                                .unwrap_or(Value::Null),
                            "scout_pack_ref": meta_map.get("scout_pack_ref").cloned().unwrap_or(Value::Null),
                            "builder_batch_ref": meta_map.get("builder_batch_ref").cloned().unwrap_or(Value::Null),
                            "plan_ref": meta_map.get("plan_ref").cloned().unwrap_or(Value::Null)
                        }),
                    );
                }
            }
            jobs_json.push(job_json);

            if needs_manager || has_error || needs_proof || stale || stalled {
                let severity = if has_error || needs_manager || needs_proof {
                    "P0"
                } else if stalled || stale {
                    "P1"
                } else {
                    "P2"
                };
                inbox_items.push(json!({
                    "severity": severity,
                    "job_id": job.id,
                    "title": job.title,
                    "status": job.status,
                    "attention": {
                        "needs_manager": needs_manager,
                        "needs_proof": needs_proof,
                        "has_error": has_error,
                        "stale": stale,
                        "stalled": stalled
                    },
                    "last": last.map(job_event_to_json).unwrap_or(Value::Null)
                }));
            }
        }

        let mut awaiting_gate = 0u64;
        let mut rejected_batches_24h = 0u64;
        for state in pipeline_slices.values_mut() {
            let Some(task_for_thread) = state.task.as_deref() else {
                continue;
            };
            let thread_id = pipeline_thread_id(task_for_thread, &state.slice_id);
            let pulled = match self.store.job_bus_pull(
                &workspace,
                bm_storage::JobBusPullRequest {
                    consumer_id: "jobs.control.center".to_string(),
                    thread_id,
                    after_seq: None,
                    limit: 40,
                },
            ) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for msg in pulled.messages {
                if !msg.kind.eq_ignore_ascii_case("gate_decision") {
                    if msg.kind.eq_ignore_ascii_case("pipeline_apply") {
                        state.apply_done = true;
                    }
                    continue;
                }
                let payload = msg
                    .payload_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                if let Some(decision) = payload.get("decision").and_then(|v| v.as_str()) {
                    state.gate_decision = Some(decision.to_ascii_lowercase());
                    state.gate_decision_ref = Some(format!(
                        "artifact://pipeline/gate/{}/{}/seq/{}",
                        task_for_thread, state.slice_id, msg.seq
                    ));
                    if decision.eq_ignore_ascii_case("reject")
                        && now_ms.saturating_sub(msg.ts_ms) <= 24 * 60 * 60 * 1000
                    {
                        rejected_batches_24h = rejected_batches_24h.saturating_add(1);
                    }
                }
            }
            if state.validator_done && state.gate_decision.is_none() {
                awaiting_gate = awaiting_gate.saturating_add(1);
            }
        }

        // Team mesh (threads + unread + edges).
        let (team_mesh, team_mesh_actions) = if self.jobs_mesh_v1_enabled {
            let consumer_id = self
                .default_agent_id
                .clone()
                .unwrap_or_else(|| "manager".to_string());

            let mut thread_ids = Vec::<String>::new();
            thread_ids.push("workspace/main".to_string());
            if let Some(task) = task_id.as_deref() {
                thread_ids.push(thread_id_for_task(task));
            }
            for job in &jobs_json {
                if let Some(id) = job.get("job_id").and_then(|v| v.as_str()) {
                    thread_ids.push(thread_id_for_job(id));
                }
            }
            let recent_threads = match self.store.job_bus_threads_recent(
                &workspace,
                bm_storage::JobBusThreadsRecentRequest { limit: 40 },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            for row in recent_threads.rows {
                thread_ids.push(row.thread_id);
            }
            thread_ids.sort();
            thread_ids.dedup();
            if thread_ids.len() > 60 {
                thread_ids.truncate(60);
            }

            let thread_statuses = match self.store.job_bus_thread_statuses(
                &workspace,
                bm_storage::JobBusThreadStatusRequest {
                    consumer_id: consumer_id.clone(),
                    thread_ids: thread_ids.clone(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let links = match self.store.job_bus_links_recent(&workspace, 20) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let edges = links
                .into_iter()
                .filter_map(|m| {
                    let payload = m.payload_json.as_deref()?;
                    let obj = serde_json::from_str::<Value>(payload).ok()?;
                    let link = obj.get("link")?.as_object()?;
                    Some(json!({
                        "seq": m.seq,
                        "from_thread": link.get("from_thread"),
                        "to_thread": link.get("to_thread"),
                        "kind": link.get("kind")
                    }))
                })
                .collect::<Vec<_>>();

            let mut thread_rows = thread_statuses
                .rows
                .into_iter()
                .filter(|r| {
                    r.last_seq.is_some() || r.unread_count > 0 || r.thread_id == "workspace/main"
                })
                .collect::<Vec<_>>();
            thread_rows.sort_by(|a, b| {
                b.unread_count
                    .cmp(&a.unread_count)
                    .then_with(|| b.last_seq.unwrap_or(0).cmp(&a.last_seq.unwrap_or(0)))
                    .then_with(|| a.thread_id.cmp(&b.thread_id))
            });

            let threads = thread_rows
                .iter()
                .map(|r| {
                    json!({
                        "thread_id": r.thread_id,
                        "after_seq": r.after_seq,
                        "unread": r.unread_count,
                        "last": {
                            "seq": r.last_seq,
                            "ts_ms": r.last_ts_ms,
                            "kind": r.last_kind,
                            "summary": r.last_summary
                        }
                    })
                })
                .collect::<Vec<_>>();

            let team_mesh_actions = thread_rows
                .iter()
                .filter(|r| r.unread_count > 0)
                .take(3)
                .map(|r| {
                    action_call(
                        "jobs.mesh.pull",
                        &format!(
                            "Pull unread messages from {} (unread={}).",
                            r.thread_id, r.unread_count
                        ),
                        "low",
                        json!({ "thread_id": r.thread_id, "limit": 50 }),
                    )
                })
                .collect::<Vec<_>>();

            (
                json!({
                    "enabled": true,
                    "consumer_id": consumer_id,
                    "threads": threads,
                    "edges": edges
                }),
                team_mesh_actions,
            )
        } else {
            (json!({ "enabled": false }), Vec::new())
        };

        // Actions (macros-first).
        let mut actions = Vec::<Value>::new();
        if !stalled_jobs.is_empty() {
            actions.push(action_call(
                "jobs.macro.rotate.stalled",
                "Rotate stalled RUNNING jobs (cancel + recreate).",
                "high",
                json!({ "stall_after_s": stall_after_s, "limit": stalled_jobs.len().min(5) }),
            ));
        }
        if !needs_manager_jobs.is_empty() {
            actions.push(action_call(
                "jobs.macro.respond.inbox",
                "Respond to manager inbox items (questions).",
                "high",
                json!({ "jobs": needs_manager_jobs, "message": "<fill>" }),
            ));
        }
        if !needs_proof_jobs.is_empty() {
            actions.push(action_call(
                "jobs.macro.enforce.proof",
                "Acknowledge proof gate by posting a manager message with proof refs.",
                "high",
                json!({ "jobs": needs_proof_jobs, "refs": ["LINK: <fill>"] }),
            ));
        }
        for state in pipeline_slices.values() {
            let Some(task_for_slice) = state.task.as_deref() else {
                continue;
            };
            if state.builder_done && !state.validator_any {
                let Some(scout_pack_ref) = state.scout_pack_ref.clone() else {
                    continue;
                };
                let Some(builder_batch_ref) = state.builder_batch_ref.clone() else {
                    continue;
                };
                let plan_ref = state
                    .plan_ref
                    .clone()
                    .unwrap_or_else(|| format!("PLAN-{}", state.slice_id));
                actions.push(action_call(
                    "jobs.macro.dispatch.validator",
                    "Builder DONE without validator: dispatch independent validator.",
                    "high",
                    json!({
                        "task": task_for_slice,
                        "slice_id": state.slice_id.clone(),
                        "scout_pack_ref": scout_pack_ref,
                        "builder_batch_ref": builder_batch_ref,
                        "plan_ref": plan_ref,
                        "executor": "claude_code",
                        "executor_profile": "audit",
                        "model": "opus-4.6"
                    }),
                ));
            }
            if state.validator_done {
                let Some(scout_pack_ref) = state.scout_pack_ref.clone() else {
                    continue;
                };
                let Some(builder_batch_ref) = state.builder_batch_ref.clone() else {
                    continue;
                };
                let Some(validator_report_ref) = state.validator_report_ref.clone() else {
                    continue;
                };
                actions.push(action_call(
                    "jobs.pipeline.gate",
                    "Validator ready: run lead gate decision.",
                    "high",
                    json!({
                        "task": task_for_slice,
                        "slice_id": state.slice_id.clone(),
                        "scout_pack_ref": scout_pack_ref,
                        "builder_batch_ref": builder_batch_ref,
                        "validator_report_ref": validator_report_ref,
                        "policy": "fail_closed"
                    }),
                ));
            }
            if state
                .gate_decision
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case("approve"))
                && !state.apply_done
            {
                let Some(decision_ref) = state.gate_decision_ref.clone() else {
                    continue;
                };
                let Some(builder_batch_ref) = state.builder_batch_ref.clone() else {
                    continue;
                };
                actions.push(action_call(
                    "jobs.pipeline.apply",
                    "Approved gate pending apply.",
                    "high",
                    json!({
                        "task": task_for_slice,
                        "slice_id": state.slice_id.clone(),
                        "decision_ref": decision_ref,
                        "builder_batch_ref": builder_batch_ref,
                        "expected_revision": state.builder_revision.unwrap_or(0)
                    }),
                ));
            }
        }
        actions.extend(team_mesh_actions);
        if actions.is_empty() {
            actions.push(action_call(
                "jobs.macro.dispatch.scout",
                "No active blockers: start scout stage (claude_code haiku deep, context-only).",
                "low",
                json!({
                    "task": "<task>",
                    "anchor": "a:<anchor>",
                    "slice_id": "SLC-001",
                    "objective": "<objective>",
                    "executor": "claude_code",
                    "model": "haiku",
                    "executor_profile": "deep"
                }),
            ));
            actions.push(action_call(
                "jobs.pipeline.gate",
                "Gate scout/builder/validator artifacts before apply.",
                "low",
                json!({
                    "task": "<task>",
                    "slice_id": "SLC-001",
                    "scout_pack_ref": "artifact://jobs/JOB-000001/scout_context_pack",
                    "builder_batch_ref": "artifact://jobs/JOB-000002/builder_diff_batch",
                    "validator_report_ref": "artifact://jobs/JOB-000003/validator_report",
                    "policy": "fail_closed"
                }),
            ));
        }

        // Defaults block (transparency).
        let defaults = json!({
            "stall_after_s": 600,
            "jobs_unknown_args_fail_closed": self.jobs_unknown_args_fail_closed_enabled,
            "jobs_strict_progress_schema": self.jobs_strict_progress_schema_enabled,
            "jobs_high_done_proof_gate": self.jobs_high_done_proof_gate_enabled,
            "jobs_wait_stream_v2": self.jobs_wait_stream_v2_enabled,
            "jobs_wait_timeout_cap_ms": 25_000,
            "jobs_mesh_v1": self.jobs_mesh_v1_enabled
        });

        let mut result = json!({
            "workspace": workspace.as_str(),
            "scope": {
                "task": task_id,
                "anchor": anchor_id
            },
            "view": view,
            "inbox": {
                "items": inbox_items,
                "count": inbox_items.len()
            },
            "execution_health": {
                "runner_status": super::runner_status_to_json(runner_status),
                "runner_leases": {
                    "count": runner_leases.runners.len(),
                    "has_more": runner_leases.has_more
                },
                "stalled_jobs": stalled_jobs.len(),
                "needs_manager": needs_manager_jobs.len(),
                "needs_proof": needs_proof_jobs.len()
            },
            "proof_health": {
                "needs_proof_jobs": needs_proof_jobs
            },
            "pipeline_health": {
                "open_scout_jobs": open_scout_jobs,
                "open_builder_jobs": open_builder_jobs,
                "open_validator_jobs": open_validator_jobs,
                "awaiting_gate": awaiting_gate,
                "rejected_batches_24h": rejected_batches_24h,
                "stale_scout_pack_count": stale_scout_pack_count
            },
            "team_mesh": team_mesh,
            "jobs": jobs_json,
            "actions": actions,
            "defaults": defaults,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);

            let (_used_jobs, trunc_jobs) = enforce_graph_list_budget(&mut result, "jobs", limit);
            let mut truncated = trunc_jobs;

            if let Some(obj) = result.as_object_mut()
                && let Some(inbox) = obj.get_mut("inbox")
            {
                let (_used_inbox, trunc_inbox) = enforce_graph_list_budget(inbox, "items", limit);
                truncated = truncated || trunc_inbox;
            }
            if let Some(obj) = result.as_object_mut()
                && let Some(mesh) = obj.get_mut("team_mesh")
            {
                let (_used_threads, trunc_threads) =
                    enforce_graph_list_budget(mesh, "threads", limit);
                truncated = truncated || trunc_threads;
                let (_used_edges, trunc_edges) = enforce_graph_list_budget(mesh, "edges", limit);
                truncated = truncated || trunc_edges;
            }

            let mut truncated_final = truncated;
            set_truncated_flag(&mut result, truncated_final);
            let used = attach_budget(&mut result, limit, truncated_final);
            if used > limit && !truncated_final {
                truncated_final = true;
                set_truncated_flag(&mut result, true);
                let _ = attach_budget(&mut result, limit, true);
                warnings.push(warning(
                    "BUDGET_OVERFLOW",
                    "payload exceeds max_chars after trimming",
                    "Increase max_chars or narrow scope/limit to reduce payload size.",
                ));
            }
            warnings.extend(budget_warnings(truncated_final, false, clamped));
        }

        // For now: keep suggestions empty; "actions" block is the primary UX.
        if warnings.is_empty() {
            ai_ok("tasks_jobs_control_center", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_control_center", result, warnings, Vec::new())
        }
    }
}
