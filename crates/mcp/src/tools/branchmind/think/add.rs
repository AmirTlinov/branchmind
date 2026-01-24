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

impl McpServer {
    pub(crate) fn tool_branchmind_think_add_typed(
        &mut self,
        args: Value,
        enforced_type: &str,
        tool_name: &'static str,
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
        if !bm_core::think::is_supported_think_card_type(enforced_type) {
            return ai_error("INVALID_INPUT", "Unsupported card.type");
        }
        let supports = match optional_string_array(args_obj, "supports") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let blocks = match optional_string_array(args_obj, "blocks") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };

        let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
        let mut parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        parsed.card_type = enforced_type.to_string();
        match apply_step_context_to_card(self, &workspace, args_obj, &mut parsed) {
            Ok(Some(w)) => warnings.push(w),
            Ok(None) => {}
            Err(resp) => return resp,
        }
        if let Err(resp) = apply_lane_context_to_card(args_obj, &mut parsed) {
            return resp;
        }

        let (branch, trace_doc, graph_doc) =
            match self.resolve_think_commit_scope(&workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (card_id, result) = match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
            workspace: &workspace,
            branch: &branch,
            trace_doc: &trace_doc,
            graph_doc: &graph_doc,
            parsed,
            supports: &supports,
            blocks: &blocks,
        }) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let trace_ref = format!("{}@{}", trace_doc, result.trace_seq);
        let graph_ref = result.last_seq.map(|seq| format!("{}@{seq}", graph_doc));
        let response = if verbosity == ResponseVerbosity::Compact {
            ai_ok(
                tool_name,
                json!({
                    "workspace": workspace.as_str(),
                    "branch": branch,
                    "card_id": card_id,
                    "inserted": result.inserted,
                    "trace_ref": trace_ref,
                    "graph_ref": graph_ref
                }),
            )
        } else {
            ai_ok(
                tool_name,
                json!({
                    "workspace": workspace.as_str(),
                    "branch": branch,
                    "trace_doc": trace_doc,
                    "graph_doc": graph_doc,
                    "card_id": card_id,
                    "inserted": result.inserted,
                    "graph_applied": {
                        "nodes_upserted": result.nodes_upserted,
                        "edges_upserted": result.edges_upserted
                    },
                    "last_seq": result.last_seq
                }),
            )
        };

        if warnings.is_empty() {
            response
        } else {
            let result = response.get("result").cloned().unwrap_or(Value::Null);
            ai_ok_with_warnings(tool_name, result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_branchmind_think_add_hypothesis(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "hypothesis", "think_add_hypothesis")
    }

    pub(crate) fn tool_branchmind_think_add_question(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "question", "think_add_question")
    }

    pub(crate) fn tool_branchmind_think_add_test(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "test", "think_add_test")
    }

    pub(crate) fn tool_branchmind_think_add_note(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "note", "think_add_note")
    }

    pub(crate) fn tool_branchmind_think_add_decision(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "decision", "think_add_decision")
    }

    pub(crate) fn tool_branchmind_think_add_evidence(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "evidence", "think_add_evidence")
    }

    pub(crate) fn tool_branchmind_think_add_frame(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "frame", "think_add_frame")
    }

    pub(crate) fn tool_branchmind_think_add_update(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "update", "think_add_update")
    }
}
