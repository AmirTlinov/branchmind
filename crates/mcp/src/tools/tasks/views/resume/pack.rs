#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_resume_pack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let events_limit = args_obj
            .get("events_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(20);
        let decisions_limit = args_obj
            .get("decisions_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(5);
        let evidence_limit = args_obj
            .get("evidence_limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(5);
        let read_only = args_obj
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let explicit_target = args_obj
            .get("task")
            .and_then(|v| v.as_str())
            .or_else(|| args_obj.get("plan").and_then(|v| v.as_str()));

        let (target_id, kind, focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (focus, focus_previous, focus_restored) = if read_only {
            (focus, None, false)
        } else {
            match restore_focus_for_explicit_target(
                &mut self.store,
                &workspace,
                explicit_target,
                focus,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        };

        let context = match build_radar_context_with_options(
            &mut self.store,
            &workspace,
            &target_id,
            kind,
            read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut events = if events_limit == 0 {
            Vec::new()
        } else {
            match self
                .store
                .list_events_for_task(&workspace, &target_id, events_limit)
            {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };
        events.reverse();
        sort_events_by_seq(&mut events);

        let (reasoning, reasoning_exists) = match resolve_reasoning_ref_for_read(
            &mut self.store,
            &workspace,
            &target_id,
            kind,
            read_only,
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut warnings = Vec::new();
        if read_only && !reasoning_exists {
            warnings.push(warning(
                "REASONING_REF_DERIVED",
                "Reasoning refs are derived because no stored ref exists for this target.",
                "Call tasks_resume_pack with read_only=false or think_pipeline to seed reasoning refs.",
            ));
        }

        let mut reasoning_branch_missing = false;

        let mut decisions = Vec::new();
        if decisions_limit > 0 {
            let slice = match self.store.graph_query(
                &workspace,
                &reasoning.branch,
                &reasoning.graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: None,
                    types: Some(vec!["decision".to_string()]),
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text: None,
                    cursor: None,
                    limit: decisions_limit,
                    include_edges: false,
                    edges_limit: 0,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownBranch) => {
                    if read_only {
                        reasoning_branch_missing = true;
                        bm_storage::GraphQuerySlice {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            next_cursor: None,
                            has_more: false,
                        }
                    } else {
                        return unknown_branch_error(&workspace);
                    }
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            decisions = graph_nodes_to_cards(slice.nodes);
        }

        let mut evidence = Vec::new();
        if evidence_limit > 0 {
            let slice = match self.store.graph_query(
                &workspace,
                &reasoning.branch,
                &reasoning.graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: None,
                    types: Some(vec!["evidence".to_string()]),
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text: None,
                    cursor: None,
                    limit: evidence_limit,
                    include_edges: false,
                    edges_limit: 0,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownBranch) => {
                    if read_only {
                        reasoning_branch_missing = true;
                        bm_storage::GraphQuerySlice {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            next_cursor: None,
                            has_more: false,
                        }
                    } else {
                        return unknown_branch_error(&workspace);
                    }
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            evidence = graph_nodes_to_cards(slice.nodes);
        }

        let blockers = context
            .radar
            .get("blockers")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let blockers_total = blockers.as_array().map(|arr| arr.len()).unwrap_or(0);
        let decisions_total = decisions.len();
        let evidence_total = evidence.len();
        let events_total = events.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "requested": {
                "task": args_obj.get("task").cloned().unwrap_or(Value::Null),
                "plan": args_obj.get("plan").cloned().unwrap_or(Value::Null)
            },
            "focus": focus,
            "target": context.target,
            "reasoning_ref": context.reasoning_ref,
            "radar": context.radar,
            "timeline": {
                "limit": events_limit,
                "events": events_to_json(events)
            },
            "signals": {
                "blockers": blockers,
                "decisions": decisions,
                "evidence": evidence,
                "stats": {
                    "blockers": blockers_total,
                    "decisions": decisions_total,
                    "evidence": evidence_total
                }
            },
            "truncated": false
        });
        if focus_restored && let Some(obj) = result.as_object_mut() {
            obj.insert("focus_restored".to_string(), Value::Bool(true));
            obj.insert(
                "focus_previous".to_string(),
                focus_previous.map(Value::String).unwrap_or(Value::Null),
            );
        }
        if let Some(steps) = context.steps
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("steps".to_string(), steps);
        }

        if reasoning_branch_missing {
            warnings.push(warning(
                "REASONING_BRANCH_MISSING",
                "Reasoning branch is missing; signals were returned empty.",
                "Seed reasoning via think_pipeline or switch read_only=false to create refs.",
            ));
        }

        self.apply_resume_pack_budget(
            &mut result,
            max_chars,
            super::budget::ResumePackBudgetContext {
                events_total,
                decisions_total,
                evidence_total,
                blockers_total,
            },
            &mut warnings,
        );

        if warnings.is_empty() {
            ai_ok("resume_pack", result)
        } else {
            ai_ok_with_warnings("resume_pack", result, warnings, Vec::new())
        }
    }
}
