#![forbid(unsafe_code)]

use super::super::graph::ThinkCardCommitInternalArgs;
use super::lane_context::apply_lane_context_to_card;
use super::step_context::apply_step_context_to_card;
use crate::*;
use serde_json::{Value, json};

fn validate_edge_node_ids(field: &str, values: &[String]) -> Result<(), Value> {
    for (idx, raw) in values.iter().enumerate() {
        match bm_core::graph::GraphNodeId::try_new(raw.clone()) {
            Ok(_) => {}
            Err(err) => {
                let recovery = match err {
                    bm_core::graph::GraphNodeIdError::TooLong
                    | bm_core::graph::GraphNodeIdError::ContainsControl => Some(
                        "supports[]/blocks[] must contain short, single-line graph ids (usually CARD-... ids). If you pasted long narrative text here by accident, move it into card.text instead.",
                    ),
                    _ => None,
                };
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("{field}[{idx}]: {}", err.message()),
                    recovery,
                    Vec::new(),
                ));
            }
        }
    }
    Ok(())
}

impl McpServer {
    pub(crate) fn tool_branchmind_think_template(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let card_type = match require_string(args_obj, "type") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let card_type = card_type.trim().to_string();
        let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
        if !bm_core::think::is_supported_think_card_type(&card_type) {
            return ai_error_with(
                "INVALID_INPUT",
                "Unsupported card type",
                Some(&format!("Supported: {}", supported.join(", "))),
                vec![suggest_call(
                    "think_template",
                    "Request a supported template type.",
                    "high",
                    json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
                )],
            );
        }

        let template = json!({
            "id": "CARD-<id>",
            "type": card_type,
            "title": null,
            "text": null,
            "status": "open",
            "tags": [],
            "meta": {}
        });

        let mut result = json!({
            "workspace": workspace.as_str(),
            "type": card_type,
            "supported_types": supported,
            "template": template,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if let Some(obj) = value.as_object_mut() {
                        changed |= obj.remove("template").is_some();
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_template", result)
        } else {
            ai_ok_with_warnings("think_template", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_branchmind_think_card(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let mut warnings = Vec::<Value>::new();

        let supports = match optional_string_array(args_obj, "supports") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let blocks = match optional_string_array(args_obj, "blocks") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        if let Err(resp) = validate_edge_node_ids("supports", &supports) {
            return resp;
        }
        if let Err(resp) = validate_edge_node_ids("blocks", &blocks) {
            return resp;
        }

        let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
        let mut parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
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
        let graph_ref = result.last_seq.map(|seq| format!("{}@{}", graph_doc, seq));

        let response = ai_ok(
            "think_card",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "trace_doc": trace_doc,
                "graph_doc": graph_doc,
                "card_id": card_id,
                "inserted": result.inserted,
                "trace_seq": result.trace_seq,
                "trace_ref": trace_ref,
                "graph_applied": {
                    "nodes_upserted": result.nodes_upserted,
                    "edges_upserted": result.edges_upserted
                },
                "last_seq": result.last_seq,
                "graph_ref": graph_ref
            }),
        );

        if warnings.is_empty() {
            response
        } else {
            let result = response.get("result").cloned().unwrap_or(Value::Null);
            ai_ok_with_warnings("think_card", result, warnings, Vec::new())
        }
    }
}
