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
use serde_json::Value;

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
                agent_id: args.agent_id.clone(),
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
                                serde_json::json!({
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

        let omit_workspace = self
            .default_workspace
            .as_deref()
            .is_some_and(|v| v == args.workspace.as_str());
        let capsule = capsule::build_handoff_capsule(capsule::HandoffCapsuleArgs {
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
