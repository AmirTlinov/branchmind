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

fn normalize_anchor_tag(raw: &str) -> Result<String, Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ai_error("INVALID_INPUT", "anchor must not be empty"));
    }
    let candidate = if raw.starts_with(ANCHOR_TAG_PREFIX) {
        raw.to_string()
    } else {
        format!("{ANCHOR_TAG_PREFIX}{raw}")
    };
    normalize_anchor_id_tag(&candidate)
        .ok_or_else(|| ai_error("INVALID_INPUT", "anchor must be a valid slug (a:<slug>)"))
}

fn normalize_key_tag(raw: &str) -> Result<String, Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ai_error("INVALID_INPUT", "key must not be empty"));
    }
    let candidate = if raw.starts_with(KEY_TAG_PREFIX) {
        raw.to_string()
    } else {
        format!("{KEY_TAG_PREFIX}{raw}")
    };
    normalize_key_id_tag(&candidate)
        .ok_or_else(|| ai_error("INVALID_INPUT", "key must be a valid slug (k:<slug>)"))
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

    pub(crate) fn tool_branchmind_think_add_knowledge(&mut self, args: Value) -> Value {
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
        let supports = match optional_string_array(args_obj, "supports") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let blocks = match optional_string_array(args_obj, "blocks") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };

        let anchor = match optional_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut key = match optional_string(args_obj, "key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let key_mode = match optional_string(args_obj, "key_mode") {
            Ok(v) => v.unwrap_or_else(|| "explicit".to_string()),
            Err(resp) => return resp,
        };
        let key_mode = key_mode.trim().to_ascii_lowercase();
        if !matches!(key_mode.as_str(), "explicit" | "auto") {
            return ai_error("INVALID_INPUT", "key_mode must be explicit|auto");
        }
        let lint_mode = match optional_string(args_obj, "lint_mode") {
            Ok(v) => v.unwrap_or_else(|| "manual".to_string()),
            Err(resp) => return resp,
        };
        let lint_mode = lint_mode.trim().to_ascii_lowercase();
        if !matches!(lint_mode.as_str(), "manual" | "auto") {
            return ai_error("INVALID_INPUT", "lint_mode must be manual|auto");
        }

        let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
        let mut parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        parsed.card_type = "knowledge".to_string();
        if key.is_none() && key_mode == "auto" {
            if anchor.as_deref().is_none() {
                return ai_error("INVALID_INPUT", "key_mode=auto requires anchor");
            }
            let source = parsed.title.as_deref().or(parsed.text.as_deref());
            let Some(source) = source else {
                return ai_error("INVALID_INPUT", "auto key requires card title or text");
            };
            let Some(slug) = crate::slugify_key(source) else {
                return ai_error("INVALID_INPUT", "auto key could not be derived");
            };
            key = Some(slug);
        }

        let mut resolved_anchor_tag: Option<String> = None;
        let mut resolved_key_tag: Option<String> = None;
        if let Some(anchor) = anchor {
            let tag = match normalize_anchor_tag(&anchor) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            resolved_anchor_tag = Some(tag.clone());
            if !tags_has(&parsed.tags, &tag) {
                parsed.tags.push(tag);
            }
        }
        if let Some(key) = key {
            let tag = match normalize_key_tag(&key) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            resolved_key_tag = Some(tag.clone());
            if !tags_has(&parsed.tags, &tag) {
                parsed.tags.push(tag);
            }
        }

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

        if lint_mode == "auto"
            && let Some(key_tag) = resolved_key_tag.as_deref()
        {
            let key_slug = key_tag
                .trim_start_matches(crate::KEY_TAG_PREFIX)
                .to_string();
            let anchor_ids = resolved_anchor_tag
                .as_ref()
                .map(|a| vec![a.clone()])
                .unwrap_or_default();
            match self.store.knowledge_keys_list_by_key(
                &workspace,
                bm_storage::KnowledgeKeysListByKeyRequest {
                    key: key_slug,
                    anchor_ids,
                    limit: 10,
                },
            ) {
                Ok(list) => {
                    let mut collisions = Vec::<String>::new();
                    for row in list.items {
                        if resolved_anchor_tag
                            .as_ref()
                            .is_some_and(|a| a == &row.anchor_id)
                        {
                            continue;
                        }
                        collisions.push(row.anchor_id);
                    }
                    if !collisions.is_empty() {
                        collisions.sort();
                        collisions.dedup();
                        warnings.push(warning(
                            "KNOWLEDGE_KEY_COLLISION",
                            &format!(
                                "key already used in other anchors: {}",
                                collisions.join(", ")
                            ),
                            "Consider a more specific key or reuse the existing anchor/key pairing.",
                        ));
                    }
                }
                Err(_) => warnings.push(warning(
                    "KNOWLEDGE_LINT_FAILED",
                    "auto lint failed to read knowledge key index",
                    "Retry with lint_mode=manual or run think.knowledge.lint separately.",
                )),
            }
        }

        let trace_ref = format!("{}@{}", trace_doc, result.trace_seq);
        let graph_ref = result.last_seq.map(|seq| format!("{}@{seq}", graph_doc));
        let response = if verbosity == ResponseVerbosity::Compact {
            ai_ok(
                "think_add_knowledge",
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
                "think_add_knowledge",
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
            ai_ok_with_warnings("think_add_knowledge", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_branchmind_think_add_frame(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "frame", "think_add_frame")
    }

    pub(crate) fn tool_branchmind_think_add_update(&mut self, args: Value) -> Value {
        self.tool_branchmind_think_add_typed(args, "update", "think_add_update")
    }
}
