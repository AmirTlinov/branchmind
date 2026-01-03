#![forbid(unsafe_code)]

use super::super::graph::ThinkCardCommitInternalArgs;
use crate::*;
use serde_json::{Value, json};

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
        if !bm_core::think::is_supported_think_card_type(enforced_type) {
            return ai_error("INVALID_INPUT", "Unsupported card.type");
        }

        let (branch, trace_doc, graph_doc) =
            match self.resolve_think_commit_scope(&workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
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
