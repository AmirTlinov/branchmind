#![forbid(unsafe_code)]

mod args;
mod capsule;
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
            target: &context.target,
            reasoning_ref: &context.reasoning_ref,
            radar: &context.radar,
            steps_summary: context.steps.as_ref(),
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

        if warnings.is_empty() {
            ai_ok("resume_super", result)
        } else {
            ai_ok_with_warnings("resume_super", result, warnings, Vec::new())
        }
    }
}
