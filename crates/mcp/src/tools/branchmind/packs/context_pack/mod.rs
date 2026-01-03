#![forbid(unsafe_code)]

mod args;
mod bridge;
mod budget;
mod read;
mod render;

use crate::*;
use serde_json::Value;

impl McpServer {
    pub(crate) fn tool_branchmind_context_pack(&mut self, args: Value) -> Value {
        let parsed = match args::parse_context_pack_args(args) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let args::ContextPackArgs {
            workspace,
            requested_target,
            requested_ref,
            notes_doc,
            trace_doc,
            graph_doc,
            notes_limit,
            trace_limit,
            limit_cards,
            decisions_limit,
            evidence_limit,
            blockers_limit,
            max_chars,
            read_only,
        } = parsed;

        let scope = match self.resolve_reasoning_scope_with_options(
            &workspace,
            ReasoningScopeInput {
                target: requested_target.clone(),
                branch: requested_ref.clone(),
                notes_doc,
                graph_doc,
                trace_doc,
            },
            read_only,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let read = match read::read_context_pack(
            self,
            read::ContextPackReadArgs {
                workspace: &workspace,
                scope: &scope,
                notes_limit,
                trace_limit,
                limit_cards,
                decisions_limit,
                evidence_limit,
                blockers_limit,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let read::ContextPackRead {
            docs,
            graph:
                read::ContextPackGraphData {
                    cards,
                    decisions,
                    evidence,
                    blockers,
                    by_type,
                    stats_by_type,
                },
            totals,
        } = read;

        let (bridge, bridge_warning) = self.maybe_build_context_pack_bridge(
            &workspace,
            requested_target.is_some(),
            &scope.branch,
            bridge::ContextPackBridgeContext {
                notes_count: totals.notes_count,
                trace_count: totals.trace_count,
                card_count: totals.cards_total,
                decisions_total: totals.decisions_total,
                evidence_total: totals.evidence_total,
                blockers_total: totals.blockers_total,
            },
        );

        let mut result = render::render_context_pack(render::ContextPackRenderArgs {
            workspace: &workspace,
            requested_target,
            requested_ref,
            scope,
            docs,
            graph: render::ContextPackRenderGraph {
                cards,
                decisions,
                evidence,
                blockers,
                by_type,
            },
            notes_limit,
            trace_limit,
            bridge,
        });

        redact_value(&mut result, 6);

        let mut warnings = Vec::new();
        if let Some(warning) = bridge_warning {
            warnings.push(warning);
        }

        self.apply_context_pack_budget(
            &mut result,
            max_chars,
            budget::ContextPackBudgetContext {
                notes_count: totals.notes_count,
                trace_count: totals.trace_count,
                cards_total: totals.cards_total,
                decisions_total: totals.decisions_total,
                evidence_total: totals.evidence_total,
                blockers_total: totals.blockers_total,
                stats_by_type: &stats_by_type,
            },
            &mut warnings,
        );

        if warnings.is_empty() {
            ai_ok("context_pack", result)
        } else {
            ai_ok_with_warnings("context_pack", result, warnings, Vec::new())
        }
    }
}
