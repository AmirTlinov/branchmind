#![forbid(unsafe_code)]

mod args;
mod capsule;
mod focus_only;
mod queries;

mod memory;
mod result;
mod signals;
mod timeline;

use crate::*;
use serde_json::{Value, json};

use queries::graph_query_or_empty;

fn job_row_to_capsule_job(job: &bm_storage::JobRow) -> Value {
    // Keep this intentionally small and copy/paste-friendly: the snapshot HUD should not
    // become a second dashboard. Deep detail lives in `tasks_jobs_open` / `open JOB-*`.
    json!({
        "id": job.id,
        "status": job.status,
        "runner": job.runner.as_deref().map(|v| truncate_string(&redact_text(v), 80)).unwrap_or_else(|| "-".to_string()),
        "summary": job.summary.as_deref().map(|v| truncate_string(&redact_text(v), 140)).unwrap_or_else(|| "-".to_string()),
        "updated_at_ms": job.updated_at_ms
    })
}

fn job_open_to_capsule_job(opened: &bm_storage::JobOpenResult) -> Value {
    // A slightly richer job hint: keep a single “latest meaningful update” so supervision
    // is cheap without opening the full job view.
    let job = &opened.job;

    // Attention is sticky across intervening progress noise.
    // - A `question` needs an explicit manager message after it.
    // - An `error` is considered resolved once a later `checkpoint` is recorded.
    let last_question_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "question")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_manager_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "manager")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_manager_proof_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "manager" && !e.refs.is_empty())
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_error_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "error")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_proof_gate_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "proof_gate")
        .map(|e| e.seq)
        .unwrap_or(0);
    let last_checkpoint_seq = opened
        .events
        .iter()
        .find(|e| e.kind == "checkpoint")
        .map(|e| e.seq)
        .unwrap_or(0);

    let needs_manager = last_question_seq > last_manager_seq;
    let has_error = last_error_seq > last_checkpoint_seq;
    let needs_proof = last_proof_gate_seq > last_checkpoint_seq.max(last_manager_proof_seq);

    let last = opened
        .events
        .iter()
        .find(|e| {
            e.kind != "heartbeat"
                && matches!(
                    e.kind.as_str(),
                    "error" | "question" | "manager" | "proof_gate"
                )
        })
        .or_else(|| {
            opened.events.iter().find(|e| {
                e.kind != "heartbeat"
                    && !e
                        .message
                        .trim_start()
                        .get(..7)
                        .is_some_and(|p| p.eq_ignore_ascii_case("runner:"))
            })
        })
        .or_else(|| opened.events.iter().find(|e| e.kind != "heartbeat"))
        .or_else(|| opened.events.first());

    let last_value = if let Some(ev) = last {
        let mut refs = ev.refs.clone();
        refs.truncate(4);
        json!({
            "ref": format!("{}@{}", ev.job_id, ev.seq),
            "seq": ev.seq,
            "ts_ms": ev.ts_ms,
            "kind": ev.kind.clone(),
            "message": truncate_string(&redact_text(&ev.message), 160),
            "refs": refs
        })
    } else {
        Value::Null
    };

    json!({
        "id": job.id.clone(),
        "status": job.status.clone(),
        "runner": job.runner.as_deref().map(|v| truncate_string(&redact_text(v), 80)).unwrap_or_else(|| "-".to_string()),
        "summary": job.summary.as_deref().map(|v| truncate_string(&redact_text(v), 140)).unwrap_or_else(|| "-".to_string()),
        "updated_at_ms": job.updated_at_ms,
        "last": last_value,
        "attention": {
            "needs_manager": needs_manager,
            "needs_proof": needs_proof,
            "has_error": has_error
        }
    })
}

fn build_meaning_map_hud(
    cards: &[Value],
    include_drafts: bool,
    focus_step_tag: Option<&str>,
    fallback_anchor: Option<String>,
) -> Value {
    use std::collections::BTreeMap;

    // Filter by visibility first (drafts hidden unless explicitly included).
    let visible_cards = cards
        .iter()
        .filter(|card| card_value_visibility_allows(card, include_drafts, focus_step_tag))
        .collect::<Vec<_>>();

    // Prefer step-scoped cards to derive the "where" anchor.
    let mut step_scoped: Vec<&Value> = Vec::new();
    if let Some(step_tag) = focus_step_tag.map(str::trim).filter(|t| !t.is_empty()) {
        for card in &visible_cards {
            let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
                continue;
            };
            if tags.iter().any(|t| t.as_str() == Some(step_tag)) {
                step_scoped.push(*card);
            }
        }
    }

    // Fallback to the full visible slice when step scope is missing.
    let source: Vec<&Value> = if !step_scoped.is_empty() {
        step_scoped
    } else {
        visible_cards
    };

    // Stats: anchor_id -> (max_ts_ms, count)
    let mut stats = BTreeMap::<String, (i64, usize)>::new();
    for card in source {
        let ts = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        for anchor in card_value_anchor_tags(card) {
            let entry = stats.entry(anchor).or_insert((0, 0));
            entry.0 = entry.0.max(ts);
            entry.1 = entry.1.saturating_add(1);
        }
    }

    let mut ranked = stats
        .into_iter()
        .map(|(id, (ts, count))| (id, ts, count))
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    let top_anchors = ranked
        .iter()
        .take(3)
        .map(|(id, _, _)| id.clone())
        .collect::<Vec<_>>();
    let (where_id, needs_anchor) = match top_anchors.first() {
        Some(id) => (id.clone(), false),
        None => (
            fallback_anchor.unwrap_or_else(|| "a:core".to_string()),
            true,
        ),
    };

    json!({ "where": where_id, "top_anchors": top_anchors, "needs_anchor": needs_anchor })
}

fn primary_engine_signal(engine: &Value) -> Option<Value> {
    let signal = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())?;
    let code = signal.get("code").cloned().unwrap_or(Value::Null);
    let severity = signal.get("severity").cloned().unwrap_or(Value::Null);
    let message = signal
        .get("message")
        .and_then(|v| v.as_str())
        .map(|v| Value::String(truncate_string(&redact_text(v), 140)))
        .unwrap_or(Value::Null);
    let refs = signal.get("refs").cloned().unwrap_or(Value::Null);
    Some(json!({
        "code": code,
        "severity": severity,
        "message": message,
        "refs": refs
    }))
}

impl McpServer {
    pub(crate) fn tool_tasks_resume_super(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let args = match args::parse_resume_super_args(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let explicit_target = args.explicit_target.as_deref();
        let focus_view = args.view;

        let (target_id, kind, focus) =
            match resolve_target_id(&mut self.store, &args.workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (focus, focus_previous, focus_restored) = if args.read_only {
            (focus, None, false)
        } else {
            match restore_focus_for_explicit_target(
                &mut self.store,
                &args.workspace,
                explicit_target,
                focus,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        };

        let context = match build_radar_context_with_options(
            &mut self.store,
            &args.workspace,
            &target_id,
            kind,
            args.read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let handoff = match build_handoff_core(&mut self.store, &args.workspace, &target_id, kind) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let first_open_step = if matches!(
            focus_view,
            args::ResumeSuperView::FocusOnly
                | args::ResumeSuperView::Smart
                | args::ResumeSuperView::Explore
                | args::ResumeSuperView::Audit
        ) && kind == TaskKind::Task
        {
            focus_only::parse_first_open_step(context.steps.as_ref())
        } else {
            None
        };
        let focus_step_tag = first_open_step
            .as_ref()
            .map(|step| step_tag_for(&step.step_id));
        let focus_step_path = first_open_step
            .as_ref()
            .and_then(|step| step.first_open.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let timeline = match timeline::load_timeline_events(
            &mut self.store,
            &args.workspace,
            &target_id,
            args.events_limit,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (reasoning, reasoning_exists) = match resolve_reasoning_ref_for_read(
            &mut self.store,
            &args.workspace,
            &target_id,
            kind,
            args.read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut warnings = Vec::new();
        if args.read_only && !reasoning_exists {
            warnings.push(warning(
                "REASONING_REF_DERIVED",
                "Reasoning refs are derived because no stored ref exists for this target.",
                "Call tasks_resume_super with read_only=false or think_pipeline to seed reasoning refs.",
            ));
        }

        let mut reasoning_branch_missing = false;
        let memory = match memory::load_resume_super_memory(
            self,
            &args.workspace,
            &reasoning,
            memory::ResumeSuperMemoryLoadArgs {
                notes_cursor: args.notes_cursor,
                notes_limit: args.notes_limit,
                trace_cursor: args.trace_cursor,
                trace_limit: args.trace_limit,
                cards_cursor: args.cards_cursor,
                cards_limit: args.cards_limit,
                focus_step_tag: focus_step_tag.clone(),
                focus_task_id: focus_step_path.as_ref().map(|_| target_id.clone()),
                focus_step_path: focus_step_path.clone(),
                view: focus_view,
                read_only: args.read_only,
            },
            &mut reasoning_branch_missing,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let signals = match signals::load_resume_super_signals(
            self,
            &args.workspace,
            &reasoning,
            signals::ResumeSuperSignalsLoadArgs {
                decisions_limit: args.decisions_limit,
                evidence_limit: args.evidence_limit,
                blockers_limit: args.blockers_limit,
                agent_id: args.agent_id.clone(),
                all_lanes: matches!(
                    focus_view,
                    args::ResumeSuperView::Full | args::ResumeSuperView::Audit
                ),
                read_only: args.read_only,
            },
            &mut reasoning_branch_missing,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if reasoning_branch_missing {
            warnings.push(warning(
                "REASONING_BRANCH_MISSING",
                "Reasoning branch is missing; memory signals were returned empty.",
                "Seed reasoning via think_pipeline or switch read_only=false to create refs.",
            ));
        }

        let mut degradation_signals = Vec::<String>::new();
        if args.read_only && !reasoning_exists {
            degradation_signals.push("reasoning_ref_derived".to_string());
        }
        if reasoning_branch_missing {
            degradation_signals.push("reasoning_branch_missing".to_string());
        }
        if memory.notes.entries.is_empty()
            && memory.trace.entries.is_empty()
            && memory.cards.is_empty()
            && signals.decisions.is_empty()
            && signals.evidence.is_empty()
            && signals.blockers.is_empty()
        {
            degradation_signals.push("context_empty".to_string());
        }
        if !memory.trace.entries.is_empty()
            && memory.notes.entries.is_empty()
            && memory.cards.is_empty()
        {
            degradation_signals.push("trace_only".to_string());
        }

        let engine = derive_reasoning_engine_step_aware(
            EngineScope {
                workspace: args.workspace.as_str(),
                branch: reasoning.branch.as_str(),
                graph_doc: reasoning.graph_doc.as_str(),
                trace_doc: reasoning.trace_doc.as_str(),
            },
            &memory.cards,
            &memory.edges,
            &memory.trace.entries,
            focus_step_tag.as_deref(),
            EngineLimits {
                signals_limit: args.engine_signals_limit,
                actions_limit: args.engine_actions_limit,
            },
        );
        let primary_signal = engine.as_ref().and_then(primary_engine_signal);

        let step_focus = if matches!(
            focus_view,
            args::ResumeSuperView::FocusOnly
                | args::ResumeSuperView::Smart
                | args::ResumeSuperView::Explore
                | args::ResumeSuperView::Audit
        ) && kind == TaskKind::Task
        {
            if let Some(first_open) = first_open_step.as_ref() {
                let lease_state = match self.store.step_lease_get(
                    &args.workspace,
                    bm_storage::StepLeaseGetRequest {
                        task_id: target_id.clone(),
                        selector: bm_storage::StepSelector {
                            step_id: Some(first_open.step_id.clone()),
                            path: None,
                        },
                    },
                ) {
                    Ok(v) => v.lease.map(|lease| (lease, v.now_seq)),
                    Err(StoreError::StepNotFound) => None,
                    Err(StoreError::UnknownId) => None,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                match self.store.step_detail(
                    &args.workspace,
                    &target_id,
                    Some(first_open.step_id.as_str()),
                    None,
                ) {
                    Ok(detail) => {
                        let mut payload = focus_only::build_step_focus_detail(
                            detail,
                            Some(&first_open.first_open),
                        );
                        if let Some((lease, now_seq)) = lease_state
                            && let Some(obj) =
                                payload.get_mut("detail").and_then(|v| v.as_object_mut())
                        {
                            obj.insert(
                                "lease".to_string(),
                                json!({
                                    "holder_agent_id": lease.holder_agent_id,
                                    "acquired_seq": lease.acquired_seq,
                                    "expires_seq": lease.expires_seq,
                                    "now_seq": now_seq
                                }),
                            );
                        }
                        Some(payload)
                    }
                    Err(StoreError::StepNotFound) => None,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            } else {
                None
            }
        } else {
            None
        };

        let graph_diff_payload = if args.include_graph_diff {
            match self.build_resume_super_graph_diff_payload(
                &args.workspace,
                &reasoning,
                reasoning_branch_missing,
                args.graph_diff_cursor,
                args.graph_diff_limit,
                &mut warnings,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else {
            None
        };

        let events_total = timeline.events.len();
        let notes_count = memory.notes.entries.len();
        let trace_count = memory.trace.entries.len();
        let cards_total = memory.cards.len();
        let blockers_total = signals.blockers.len();
        let decisions_total = signals.decisions.len();
        let evidence_total = signals.evidence.len();
        let stats_by_type = memory.stats_by_type.clone();

        let lane_summary = if focus_view == args::ResumeSuperView::Audit {
            let mut cards = Vec::<Value>::new();
            cards.extend(memory.cards.iter().cloned());
            cards.extend(signals.decisions.iter().cloned());
            cards.extend(signals.evidence.iter().cloned());
            cards.extend(signals.blockers.iter().cloned());
            Some(build_lane_summary(&cards, 8))
        } else {
            None
        };

        let include_drafts = focus_view == args::ResumeSuperView::Audit;
        let fallback_anchor = context
            .target
            .get("title")
            .and_then(|v| v.as_str())
            .and_then(|title| capsule::suggested_anchor_title(Some(title)))
            .or_else(|| {
                context
                    .target
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
            })
            .map(|title| capsule::derive_anchor_id_from_title(&title));
        let map_hud = if let Some(step_tag) = focus_step_tag.as_deref() {
            // Robustness: derive the "where" compass from an explicit step-scoped query rather
            // than relying on the relevance-selected memory slice to contain the latest step note.
            let types = bm_core::think::SUPPORTED_THINK_CARD_TYPES
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>();
            let step_slice = match graph_query_or_empty(
                self,
                &args.workspace,
                &reasoning.branch,
                &reasoning.graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: None,
                    types: Some(types),
                    // Do not filter by status: step-scoped notes often omit `status`, but still
                    // define the "where" anchor for the map HUD.
                    status: None,
                    tags_any: None,
                    tags_all: Some(vec![step_tag.to_string()]),
                    text: None,
                    cursor: None,
                    limit: 20,
                    include_edges: false,
                    edges_limit: 0,
                },
                args.read_only,
                &mut reasoning_branch_missing,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let step_cards = graph_nodes_to_cards(step_slice.nodes);
            if step_cards.is_empty() {
                build_meaning_map_hud(
                    &memory.cards,
                    include_drafts,
                    Some(step_tag),
                    fallback_anchor.clone(),
                )
            } else {
                build_meaning_map_hud(
                    &step_cards,
                    include_drafts,
                    Some(step_tag),
                    fallback_anchor.clone(),
                )
            }
        } else {
            build_meaning_map_hud(&memory.cards, include_drafts, None, fallback_anchor.clone())
        };

        let omit_workspace = self
            .default_workspace
            .as_deref()
            .is_some_and(|v| v == args.workspace.as_str());

        // Delegation UX: surface at most one active job (RUNNING > QUEUED) for the focused task.
        // This is a low-noise hint; detailed job history is available via explicit tools.
        let active_job = if target_id.starts_with("TASK-") {
            let running = match self.store.jobs_list(
                &args.workspace,
                bm_storage::JobsListRequest {
                    status: Some("RUNNING".to_string()),
                    task_id: Some(target_id.clone()),
                    anchor_id: None,
                    limit: 1,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            if let Some(job) = running.jobs.into_iter().next() {
                Some(job)
            } else {
                let queued = match self.store.jobs_list(
                    &args.workspace,
                    bm_storage::JobsListRequest {
                        status: Some("QUEUED".to_string()),
                        task_id: Some(target_id.clone()),
                        anchor_id: None,
                        limit: 1,
                    },
                ) {
                    Ok(v) => v,
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                queued.jobs.into_iter().next()
            }
        } else {
            None
        };

        let active_job_open = if let Some(job) = active_job.as_ref() {
            match self.store.job_open(
                &args.workspace,
                bm_storage::JobOpenRequest {
                    id: job.id.clone(),
                    include_prompt: false,
                    include_events: true,
                    include_meta: false,
                    max_events: 20,
                    before_seq: None,
                },
            ) {
                Ok(v) => Some(v),
                Err(StoreError::UnknownId) => None,
                Err(StoreError::InvalidInput(_)) => None,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            None
        };

        const DEFAULT_TASK_STALE_AFTER_MS: i64 = 14 * 24 * 60 * 60 * 1000;
        let now_ms = crate::support::now_ms_i64();
        let plan_horizon = if kind == TaskKind::Plan && target_id.starts_with("PLAN-") {
            match self.store.plan_horizon_stats_for_plan(
                &args.workspace,
                &target_id,
                now_ms,
                DEFAULT_TASK_STALE_AFTER_MS,
            ) {
                Ok(stats) => {
                    let active = stats.active.max(0) as u64;
                    let backlog = stats.backlog.max(0) as u64;
                    let parked = stats.parked.max(0) as u64;
                    let stale = stats.stale.max(0) as u64;
                    let done = stats.done.max(0) as u64;
                    let total = stats.total.max(0) as u64;

                    let mut horizon = json!({
                        "active": active,
                        "backlog": backlog,
                        "parked": parked,
                        "stale": stale,
                        "done": done,
                        "total": total,
                        "active_limit": 3u64,
                        "over_active_limit": active > 3
                    });
                    if let Some(wake) = stats.next_wake
                        && let Some(obj) = horizon.as_object_mut()
                    {
                        obj.insert(
                            "next_wake".to_string(),
                            json!({
                                "task": wake.task_id,
                                "parked_until_ts_ms": wake.parked_until_ts_ms
                            }),
                        );
                    }
                    Some(horizon)
                }
                Err(StoreError::InvalidInput(_)) => None,
                Err(StoreError::UnknownId) => None,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            None
        };

        // Delegation UX (glanceable): always provide an explicit inbox + runner liveness summary.
        // This must be derived from persisted state (leases, job rows) — no heuristics.
        let inbox_counts = match self.store.jobs_status_counts(&args.workspace) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let inbox = json!({
            "running": inbox_counts.running,
            "queued": inbox_counts.queued
        });

        let runner_status = match self.store.runner_status_snapshot(&args.workspace, now_ms) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let runner_is_offline = runner_status.status == "offline";
        let runner_autostart_active = self.maybe_autostart_runner(
            &args.workspace,
            now_ms,
            inbox_counts.queued as usize,
            runner_is_offline,
        );
        let runner_status_json = json!({
            "status": runner_status.status,
            "live_count": runner_status.live_count as u64,
            "idle_count": runner_status.idle_count as u64,
            "offline_count": runner_status.offline_count as u64,
            "runner_id": runner_status.runner_id,
            "active_job_id": runner_status.active_job_id,
            "lease_expires_at_ms": runner_status.lease_expires_at_ms
        });

        // When jobs are queued and no runner lease is active, provide a hunt-free copy/paste
        // runner start hint (same as jobs_radar).
        let runner_bootstrap = if inbox_counts.queued > 0
            && !runner_autostart_active
            && runner_status_json
                .get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s == "offline")
        {
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
                args.workspace.as_str(),
                mcp_bin.to_string_lossy()
            );
            Some(json!({ "cmd": cmd }))
        } else {
            None
        };

        let mut capsule = capsule::build_handoff_capsule(capsule::HandoffCapsuleArgs {
            toolset: self.toolset,
            workspace: &args.workspace,
            omit_workspace,
            kind,
            focus: focus.as_deref(),
            agent_id: args.agent_id.as_deref(),
            audit_all_lanes: focus_view == args::ResumeSuperView::Audit,
            target: &context.target,
            reasoning_ref: &context.reasoning_ref,
            radar: &context.radar,
            steps_summary: context.steps.as_ref(),
            step_focus: step_focus.as_ref(),
            map_hud,
            primary_signal,
            handoff: &handoff,
            timeline: &timeline,
            notes_count,
            notes_has_more: memory.notes.has_more,
            trace_count,
            trace_has_more: memory.trace.has_more,
            cards_count: cards_total,
            cards_has_more: memory.cards_has_more,
            blockers_total,
            decisions_total,
            evidence_total,
            graph_diff_payload: graph_diff_payload.as_ref(),
        });
        if let Some(obj) = capsule.as_object_mut()
            && let Some(where_obj) = obj.get_mut("where").and_then(|v| v.as_object_mut())
        {
            if let Some(horizon) = plan_horizon.as_ref() {
                where_obj.insert("horizon".to_string(), horizon.clone());
            }
            where_obj.insert("inbox".to_string(), inbox);
            where_obj.insert("runner_status".to_string(), runner_status_json);
            if let Some(runner_bootstrap) = runner_bootstrap {
                where_obj.insert("runner_bootstrap".to_string(), runner_bootstrap);
            }
            if let Some(opened) = active_job_open.as_ref() {
                where_obj.insert("job".to_string(), job_open_to_capsule_job(opened));
            } else if let Some(job) = active_job.as_ref() {
                where_obj.insert("job".to_string(), job_row_to_capsule_job(job));
            }

            // Second-brain core: surface a stable mindpack ref (when present) so /compact or
            // restarts always have a cheap “resume by meaning” handle.
            if let Ok(Some(checkout)) = self.store.branch_checkout_get(&args.workspace)
                && let Ok(slice) =
                    self.store
                        .doc_show_tail(&args.workspace, &checkout, "mindpack", None, 1)
                && let Some(entry) = slice.entries.last()
            {
                where_obj.insert(
                    "pack".to_string(),
                    json!({ "ref": format!("mindpack@{}", entry.seq) }),
                );
            }
        }

        let mut result = result::build_resume_super_result(result::ResumeSuperResultArgs {
            workspace: &args.workspace,
            args_obj,
            notes_cursor: args.notes_cursor,
            notes_limit: args.notes_limit,
            trace_cursor: args.trace_cursor,
            trace_limit: args.trace_limit,
            focus,
            focus_previous,
            focus_restored,
            context,
            timeline,
            signals,
            memory,
            include_graph_diff: args.include_graph_diff,
            graph_diff_payload,
            degradation_signals: &degradation_signals,
        });
        if let Some(obj) = result.as_object_mut() {
            obj.insert("capsule".to_string(), capsule);
            if let Some(engine) = engine {
                obj.insert("engine".to_string(), engine);
            }
            if let Some(step_focus) = step_focus {
                obj.insert("step_focus".to_string(), step_focus);
            }
            if let Some(lane_summary) = lane_summary {
                obj.insert("lane_summary".to_string(), lane_summary);
            }
        }

        self.apply_resume_super_budget(
            &mut result,
            args.max_chars,
            super::budget::ResumeSuperBudgetContext {
                events_total,
                notes_count,
                trace_count,
                cards_total,
                stats_by_type: &stats_by_type,
            },
            &mut degradation_signals,
            &mut warnings,
        );

        if focus_view == args::ResumeSuperView::FocusOnly {
            let step_path = result
                .get("steps")
                .and_then(|v| v.get("first_open"))
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            focus_only::apply_focus_only_shaping(
                &mut result,
                step_path.as_deref(),
                focus_step_tag.as_deref(),
                12,
                args.include_graph_diff,
            );
        }

        if warnings.is_empty() {
            ai_ok("resume_super", result)
        } else {
            ai_ok_with_warnings("resume_super", result, warnings, Vec::new())
        }
    }
}
