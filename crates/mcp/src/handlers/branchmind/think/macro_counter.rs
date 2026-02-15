#![forbid(unsafe_code)]

use super::super::graph::ThinkCardCommitInternalArgs;
use super::lane_context::apply_lane_context_to_card;
use super::step_context::apply_step_context_to_card;
use crate::*;
use serde_json::{Value, json};

fn parse_response_verbosity(
    args_obj: &serde_json::Map<String, Value>,
    fallback: ResponseVerbosity,
) -> Result<ResponseVerbosity, Value> {
    let raw = match optional_string(args_obj, "verbosity")? {
        Some(v) => v,
        None => return Ok(fallback),
    };
    let trimmed = raw.trim();
    ResponseVerbosity::from_str(trimmed)
        .ok_or_else(|| ai_error("INVALID_INPUT", "verbosity must be one of: full|compact"))
}

fn validate_card_id(field: &str, raw: &str) -> Result<(), Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} must not be empty"),
        ));
    }
    match bm_core::graph::GraphNodeId::try_new(trimmed.to_string()) {
        Ok(_) => Ok(()),
        Err(err) => Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field}: {}", err.message()),
            Some(
                "Use a stable graph id (usually CARD-... from the step snapshot), not free-form text.",
            ),
            Vec::new(),
        )),
    }
}

fn default_counter_card_value(label: &str) -> Value {
    json!({
        "type": "hypothesis",
        "title": format!("Counter-hypothesis: {label}"),
        "text": "Steelman the opposite case; include 1 disconfirming test idea.",
        "status": "open",
        "tags": ["bm7", "counter"]
    })
}

fn default_counter_test_value(label: &str) -> Value {
    json!({
        "type": "test",
        "title": format!("Test: Counter-hypothesis: {label}"),
        "text": "Define the smallest runnable check for this counter-position.",
        "status": "open",
        "tags": ["bm4"]
    })
}

impl McpServer {
    /// One-shot DX macro:
    /// - create a counter-hypothesis (tagged `counter`) that blocks `against`
    /// - create a test stub that supports the counter-hypothesis
    pub(crate) fn tool_branchmind_think_macro_counter_hypothesis_stub(
        &mut self,
        args: Value,
    ) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let mut warnings = Vec::<Value>::new();
        let verbosity = match parse_response_verbosity(args_obj, self.response_verbosity) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let against = match require_string(args_obj, "against") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Err(resp) = validate_card_id("against", &against) {
            return resp;
        }
        let label = match optional_string(args_obj, "label") {
            Ok(v) => v.unwrap_or_else(|| against.trim().to_string()),
            Err(resp) => return resp,
        };

        // Scope resolution is shared between both commits (same branch + docs).
        let (branch, trace_doc, graph_doc) =
            match self.resolve_think_commit_scope(&workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        // ===== Counter-hypothesis =====
        let counter_value = args_obj
            .get("counter")
            .cloned()
            .unwrap_or_else(|| default_counter_card_value(&label));
        let mut counter_parsed = match parse_think_card(&workspace, counter_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        counter_parsed.card_type = "hypothesis".to_string();
        if !tags_has(&counter_parsed.tags, "counter") {
            counter_parsed.tags.push("counter".to_string());
        }
        match apply_step_context_to_card(self, &workspace, args_obj, &mut counter_parsed) {
            Ok(Some(w)) => warnings.push(w),
            Ok(None) => {}
            Err(resp) => return resp,
        }
        if let Err(resp) = apply_lane_context_to_card(args_obj, &mut counter_parsed) {
            return resp;
        }
        let blocks = vec![against.trim().to_string()];
        let supports: Vec<String> = Vec::new();

        // ===== Test stub (supports counter-hypothesis) =====
        // Parse + normalize before committing anything so invalid user input can't leave
        // a dangling counter-position card without its paired test stub.
        let test_value = args_obj
            .get("test")
            .cloned()
            .unwrap_or_else(|| default_counter_test_value(&label));
        let mut test_parsed = match parse_think_card(&workspace, test_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        test_parsed.card_type = "test".to_string();
        match apply_step_context_to_card(self, &workspace, args_obj, &mut test_parsed) {
            Ok(Some(w)) => warnings.push(w),
            Ok(None) => {}
            Err(resp) => return resp,
        }
        if let Err(resp) = apply_lane_context_to_card(args_obj, &mut test_parsed) {
            return resp;
        }

        let (counter_id, counter_result) =
            match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
                workspace: &workspace,
                branch: &branch,
                trace_doc: &trace_doc,
                graph_doc: &graph_doc,
                parsed: counter_parsed,
                supports: &supports,
                blocks: &blocks,
            }) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        let counter_trace_ref = format!("{trace_doc}@{}", counter_result.trace_seq);
        let counter_graph_ref = counter_result
            .last_seq
            .map(|seq| format!("{graph_doc}@{seq}"));

        let supports = vec![counter_id.clone()];
        let blocks: Vec<String> = Vec::new();

        let (test_id, test_result) =
            match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
                workspace: &workspace,
                branch: &branch,
                trace_doc: &trace_doc,
                graph_doc: &graph_doc,
                parsed: test_parsed,
                supports: &supports,
                blocks: &blocks,
            }) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        let test_trace_ref = format!("{trace_doc}@{}", test_result.trace_seq);
        let test_graph_ref = test_result.last_seq.map(|seq| format!("{graph_doc}@{seq}"));

        let result = if verbosity == ResponseVerbosity::Compact {
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "counter": {
                    "card_id": counter_id,
                    "inserted": counter_result.inserted,
                    "trace_ref": counter_trace_ref,
                    "graph_ref": counter_graph_ref
                },
                "test": {
                    "card_id": test_id,
                    "inserted": test_result.inserted,
                    "trace_ref": test_trace_ref,
                    "graph_ref": test_graph_ref
                }
            })
        } else {
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "trace_doc": trace_doc,
                "graph_doc": graph_doc,
                "counter": {
                    "card_id": counter_id,
                    "inserted": counter_result.inserted,
                    "trace_seq": counter_result.trace_seq,
                    "trace_ref": counter_trace_ref,
                    "graph_applied": {
                        "nodes_upserted": counter_result.nodes_upserted,
                        "edges_upserted": counter_result.edges_upserted
                    },
                    "last_seq": counter_result.last_seq,
                    "graph_ref": counter_graph_ref
                },
                "test": {
                    "card_id": test_id,
                    "inserted": test_result.inserted,
                    "trace_seq": test_result.trace_seq,
                    "trace_ref": test_trace_ref,
                    "graph_applied": {
                        "nodes_upserted": test_result.nodes_upserted,
                        "edges_upserted": test_result.edges_upserted
                    },
                    "last_seq": test_result.last_seq,
                    "graph_ref": test_graph_ref
                }
            })
        };

        if warnings.is_empty() {
            ai_ok("think_macro_counter_hypothesis_stub", result)
        } else {
            ai_ok_with_warnings(
                "think_macro_counter_hypothesis_stub",
                result,
                warnings,
                Vec::new(),
            )
        }
    }
}
