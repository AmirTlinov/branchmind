#![forbid(unsafe_code)]

mod args;
mod bridge;
mod budget;
mod capsule;
mod read;
mod render;

use crate::*;
use serde_json::Value;

impl McpServer {
    pub(crate) fn tool_branchmind_context_pack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let parsed = match args::parse_context_pack_args(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let args::ContextPackArgs {
            workspace,
            warm_archive,
            requested_target,
            requested_ref,
            notes_doc,
            trace_doc,
            graph_doc,
            step,
            agent_id,
            all_lanes,
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

        let step_ctx = if let Some(step_raw) = step.as_deref() {
            match crate::tools::branchmind::think::resolve_step_context_from_args(
                self, &workspace, args_obj, step_raw,
            ) {
                Ok(v) => Some(v),
                Err(resp) => return resp,
            }
        } else {
            None
        };
        let focus_step_tag = step_ctx.as_ref().map(|ctx| ctx.step_tag.as_str());
        let focus_task_id = step_ctx.as_ref().map(|ctx| ctx.task_id.as_str());
        let focus_step_path = step_ctx.as_ref().map(|ctx| ctx.step.path.as_str());

        let read = match read::read_context_pack(
            self,
            read::ContextPackReadArgs {
                workspace: &workspace,
                scope: &scope,
                agent_id: agent_id.as_deref(),
                all_lanes,
                warm_archive,
                notes_limit,
                trace_limit,
                limit_cards,
                decisions_limit,
                evidence_limit,
                blockers_limit,
                focus_step_tag,
                focus_task_id,
                focus_step_path,
                read_only,
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

        // Derived: a compact reasoning engine output (signals + actions) over the returned slice.
        let mut engine_cards = Vec::<Value>::new();
        engine_cards.extend(cards.iter().cloned());
        engine_cards.extend(decisions.iter().cloned());
        engine_cards.extend(evidence.iter().cloned());
        engine_cards.extend(blockers.iter().cloned());
        let mut engine_ids = Vec::<String>::new();
        {
            let mut seen = std::collections::BTreeSet::<String>::new();
            for card in &engine_cards {
                let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                    continue;
                };
                if seen.insert(id.to_string()) {
                    engine_ids.push(id.to_string());
                }
            }
        }
        let edges = if engine_ids.is_empty() {
            Vec::new()
        } else {
            match self.store.graph_query(
                &workspace,
                &scope.branch,
                &scope.graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: Some(engine_ids.clone()),
                    types: None,
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text: None,
                    cursor: None,
                    limit: engine_ids.len().max(1),
                    include_edges: true,
                    edges_limit: (engine_ids.len().saturating_mul(6)).min(200),
                },
            ) {
                Ok(v) => graph_edges_to_json(v.edges),
                Err(StoreError::UnknownBranch) => return unknown_branch_error(&workspace),
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };
        let engine = derive_reasoning_engine_step_aware(
            EngineScope {
                workspace: workspace.as_str(),
                branch: scope.branch.as_str(),
                graph_doc: scope.graph_doc.as_str(),
                trace_doc: scope.trace_doc.as_str(),
            },
            &engine_cards,
            &edges,
            &docs.trace.entries,
            focus_step_tag,
            EngineLimits {
                signals_limit: 6,
                actions_limit: 2,
            },
        );

        let capsule = capsule::build_context_pack_capsule(capsule::ContextPackCapsuleArgs {
            workspace: &workspace,
            branch: scope.branch.as_str(),
            notes_doc: scope.notes_doc.as_str(),
            trace_doc: scope.trace_doc.as_str(),
            graph_doc: scope.graph_doc.as_str(),
            agent_id: agent_id.as_deref(),
            all_lanes,
            step_ctx: step_ctx.as_ref(),
            engine: engine.as_ref(),
        });

        let lane_summary = if all_lanes {
            Some(build_lane_summary(&engine_cards, 8))
        } else {
            None
        };

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

        if let Some(obj) = result.as_object_mut() {
            obj.insert("capsule".to_string(), capsule);
            if let Some(engine) = engine {
                obj.insert("engine".to_string(), engine);
            }
            if let Some(lane_summary) = lane_summary {
                obj.insert("lane_summary".to_string(), lane_summary);
            }
        }

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

        // Post-budget cleanup: keep derived graphs/actions aligned to the returned slice.
        let entries_snapshot = result
            .get("trace")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(sequential) = result
            .get_mut("trace")
            .and_then(|v| v.get_mut("sequential"))
        {
            filter_trace_sequential_graph_to_entries(sequential, &entries_snapshot);
        }

        let mut cards_snapshot = Vec::<Value>::new();
        if let Some(arr) = result.get("cards").and_then(|v| v.as_array()) {
            cards_snapshot.extend(arr.iter().cloned());
        }
        for path in [
            &["signals", "decisions"][..],
            &["signals", "evidence"][..],
            &["signals", "blockers"][..],
        ] {
            if let Some(arr) = result
                .get(path[0])
                .and_then(|v| v.get(path[1]))
                .and_then(|v| v.as_array())
            {
                cards_snapshot.extend(arr.iter().cloned());
            }
        }
        if let Some(engine) = result.get_mut("engine") {
            filter_engine_to_cards(engine, &cards_snapshot);
        }
        if let Some(capsule) = result.get_mut("capsule") {
            capsule::filter_context_pack_capsule_to_cards(capsule, &cards_snapshot);
        }

        if warnings.is_empty() {
            ai_ok("context_pack", result)
        } else {
            ai_ok_with_warnings("context_pack", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_branchmind_context_pack_export(&mut self, args: Value) -> Value {
        use std::fs;
        use std::path::Path;

        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let out_file = match require_string(args_obj, "out_file") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if out_file.trim().is_empty() {
            return ai_error("INVALID_INPUT", "out_file must not be empty");
        }

        let pack = self.tool_branchmind_context_pack(args);
        let ok = pack
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !ok {
            return pack;
        }
        let Some(pack_result) = pack.get("result") else {
            return ai_error("INTERNAL", "context_pack returned no result");
        };

        let bytes = match serde_json::to_vec(pack_result) {
            Ok(v) => v,
            Err(err) => return ai_error("INTERNAL", &format!("Failed to serialize result: {err}")),
        };

        let out_path = Path::new(out_file.as_str());
        if out_path.is_dir() {
            return ai_error(
                "INVALID_INPUT",
                "out_file must be a file path (got a directory)",
            );
        }
        if let Some(parent) = out_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            return ai_error(
                "IO_ERROR",
                &format!("Failed to create parent directories: {err}"),
            );
        }

        let tmp_path = out_path.with_extension(format!("tmp.{}", std::process::id()));
        if let Err(err) = fs::write(&tmp_path, &bytes) {
            return ai_error("IO_ERROR", &format!("Failed to write temp file: {err}"));
        }
        if out_path.exists() {
            let _ = fs::remove_file(out_path);
        }
        if let Err(err) = fs::rename(&tmp_path, out_path) {
            let _ = fs::remove_file(&tmp_path);
            return ai_error(
                "IO_ERROR",
                &format!("Failed to move temp file into place: {err}"),
            );
        }

        let truncated = pack_result
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ai_ok(
            "context_pack_export",
            serde_json::json!({
                "out_file": out_file,
                "bytes": bytes.len(),
                "truncated": truncated
            }),
        )
    }
}
