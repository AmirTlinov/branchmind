#![forbid(unsafe_code)]

use super::{ANCHORS_GRAPH_DOC, ANCHORS_TRACE_DOC};
use crate::*;
use serde_json::{Value, json};

fn normalize_visibility_tag(raw: Option<String>, card_type: &str) -> Result<String, Value> {
    let default = match card_type.trim().to_ascii_lowercase().as_str() {
        "decision" | "evidence" | "test" => "canon",
        _ => "draft",
    };
    let v = raw.unwrap_or_else(|| default.to_string());
    let v = v.trim().to_ascii_lowercase();
    match v.as_str() {
        "draft" => Ok("v:draft".to_string()),
        "canon" => Ok("v:canon".to_string()),
        _ => Err(ai_error(
            "INVALID_INPUT",
            "visibility must be one of: draft, canon",
        )),
    }
}

impl McpServer {
    pub(crate) fn tool_branchmind_macro_anchor_note(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let anchor_id = match require_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let content = match require_string(args_obj, "content") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let card_type = match optional_string(args_obj, "card_type") {
            Ok(v) => v.unwrap_or_else(|| "note".to_string()),
            Err(resp) => return resp,
        };
        let card_type = card_type.trim().to_string();
        if !bm_core::think::is_supported_think_card_type(&card_type) {
            let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
            return ai_error_with(
                "INVALID_INPUT",
                "Unsupported card.type",
                Some(&format!("Supported: {}", supported.join(", "))),
                vec![suggest_call(
                    "think_template",
                    "Get a valid card skeleton.",
                    "high",
                    json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
                )],
            );
        }
        let pin = match optional_bool(args_obj, "pin") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let visibility = match optional_string(args_obj, "visibility") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let visibility_tag = match normalize_visibility_tag(visibility, &card_type) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let step_raw = match optional_string(args_obj, "step") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let target_raw = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let mut warnings = Vec::<Value>::new();

        // Anchor ids can evolve over time. If the requested id is an alias, resolve it to the
        // canonical anchor id for this workspace.
        let mut effective_anchor_id = anchor_id.clone();
        let mut existing = match self.store.anchor_get(
            &workspace,
            bm_storage::AnchorGetRequest {
                id: effective_anchor_id.clone(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if existing.is_none() {
            let resolved = match self
                .store
                .anchor_resolve_id(&workspace, &effective_anchor_id)
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            if let Some(canonical) = resolved {
                warnings.push(warning(
                    "ANCHOR_ALIAS_RESOLVED",
                    "anchor id resolved via alias mapping",
                    "Use the canonical anchor id for new work; the snapshot includes alias-tagged history automatically.",
                ));
                effective_anchor_id = canonical.clone();
                existing = match self
                    .store
                    .anchor_get(&workspace, bm_storage::AnchorGetRequest { id: canonical })
                {
                    Ok(v) => v,
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
            }
        }

        let title_present = args_obj.contains_key("title");
        let kind_present = args_obj.contains_key("kind");
        let status_present = args_obj.contains_key("status");
        let description_override = match optional_nullable_string(args_obj, "description") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs_override = match optional_string_array(args_obj, "refs") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let aliases_override = match optional_string_array(args_obj, "aliases") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_override = match optional_nullable_string(args_obj, "parent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let depends_override = match optional_string_array(args_obj, "depends_on") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let title = match optional_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let kind = match optional_string(args_obj, "kind") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let status = match optional_string(args_obj, "status") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (title, kind, description, refs, aliases, parent_id, depends_on, status) =
            match existing {
                Some(existing) => {
                    let title = title
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(existing.title);
                    let kind = kind
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(existing.kind);
                    let status = status
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(existing.status);
                    let description = match description_override {
                        Some(v) => v.filter(|s| !s.trim().is_empty()),
                        None => existing.description,
                    };
                    let refs = refs_override.unwrap_or(existing.refs);
                    let aliases = aliases_override.unwrap_or(existing.aliases);
                    let parent_id = match parent_override {
                        Some(v) => v.filter(|s| !s.trim().is_empty()),
                        None => existing.parent_id,
                    };
                    let depends_on = depends_override.unwrap_or(existing.depends_on);
                    (
                        title,
                        kind,
                        description,
                        refs,
                        aliases,
                        parent_id,
                        depends_on,
                        status,
                    )
                }
                None => {
                    let title = title.filter(|s| !s.trim().is_empty());
                    let kind = kind.filter(|s| !s.trim().is_empty());
                    if !title_present || title.is_none() {
                        return ai_error(
                            "INVALID_INPUT",
                            "title is required when creating a new anchor",
                        );
                    }
                    if !kind_present || kind.is_none() {
                        return ai_error(
                            "INVALID_INPUT",
                            "kind is required when creating a new anchor",
                        );
                    }
                    let status = if status_present {
                        status
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or("active".to_string())
                    } else {
                        "active".to_string()
                    };
                    let description = description_override
                        .flatten()
                        .filter(|s| !s.trim().is_empty());
                    let refs = refs_override.unwrap_or_default();
                    let aliases = aliases_override.unwrap_or_default();
                    let parent_id = parent_override.flatten().filter(|s| !s.trim().is_empty());
                    let depends_on = depends_override.unwrap_or_default();
                    (
                        title.unwrap(),
                        kind.unwrap(),
                        description,
                        refs,
                        aliases,
                        parent_id,
                        depends_on,
                        status,
                    )
                }
            };

        let upsert = match self.store.anchor_upsert(
            &workspace,
            bm_storage::AnchorUpsertRequest {
                id: effective_anchor_id.clone(),
                title: title.clone(),
                kind: kind.clone(),
                description: description.clone(),
                refs: refs.clone(),
                aliases: aliases.clone(),
                parent_id: parent_id.clone(),
                depends_on: depends_on.clone(),
                status: status.clone(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Resolve optional step context (and choose the target scope when possible).
        let mut step_tag: Option<String> = None;
        let mut step_meta: Option<Value> = None;
        let mut commit_target: Option<String> = target_raw.clone();

        if let Some(step_raw) = step_raw.as_deref() {
            match crate::tools::branchmind::think::resolve_step_context_from_args(
                self, &workspace, args_obj, step_raw,
            ) {
                Ok(Some(ctx)) => {
                    commit_target = Some(ctx.task_id.clone());
                    step_tag = Some(ctx.step_tag);
                    step_meta = Some(json!({
                        "task_id": ctx.task_id,
                        "step_id": ctx.step.step_id,
                        "path": ctx.step.path
                    }));
                }
                Ok(None) => {
                    warnings.push(warning(
                        "STEP_CONTEXT_IGNORED",
                        "step was ignored (no TASK focus/target)",
                        "Set workspace focus to a TASK (tasks_focus_set) or pass target=TASK-... to bind step-scoped cards.",
                    ));
                }
                Err(resp) => return resp,
            }
        }

        // Choose commit scope:
        // - If a target/step is provided, commit into that entity's reasoning scope (so step tags are discoverable from the task).
        // - Otherwise, commit into the workspace-level anchors registry scope.
        let (commit_branch, commit_trace_doc, commit_graph_doc) = match commit_target {
            Some(target) => {
                let scope = match self.resolve_reasoning_scope(
                    &workspace,
                    ReasoningScopeInput {
                        target: Some(target),
                        branch: None,
                        notes_doc: None,
                        graph_doc: None,
                        trace_doc: None,
                    },
                ) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                (scope.branch, scope.trace_doc, scope.graph_doc)
            }
            None => {
                let checkout = match require_checkout_branch(&mut self.store, &workspace) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                (
                    checkout,
                    ANCHORS_TRACE_DOC.to_string(),
                    ANCHORS_GRAPH_DOC.to_string(),
                )
            }
        };

        let anchor_tag = upsert.anchor.id.clone();
        let mut tags = vec![anchor_tag.clone(), visibility_tag];
        if pin {
            tags.push(PIN_TAG.to_string());
        }
        if let Some(step_tag) = step_tag.as_ref() {
            tags.push(step_tag.clone());
        }

        let mut meta = serde_json::Map::new();
        meta.insert("anchor".to_string(), Value::String(anchor_tag.clone()));
        if let Some(step_meta) = step_meta {
            meta.insert("step".to_string(), step_meta);
        }

        let card_value = json!({
            "type": card_type,
            "text": content,
            "status": "open",
            "tags": tags,
            "meta": Value::Object(meta)
        });
        let parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let supports: Vec<String> = Vec::new();
        let blocks: Vec<String> = Vec::new();

        let (card_id, commit) = match self.commit_think_card_internal(
            super::super::graph::ThinkCardCommitInternalArgs {
                workspace: &workspace,
                branch: &commit_branch,
                trace_doc: &commit_trace_doc,
                graph_doc: &commit_graph_doc,
                parsed,
                supports: &supports,
                blocks: &blocks,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "anchor": {
                "id": upsert.anchor.id,
                "title": upsert.anchor.title,
                "kind": upsert.anchor.kind,
                "status": upsert.anchor.status,
                "description": upsert.anchor.description,
                "refs": upsert.anchor.refs,
                "aliases": upsert.anchor.aliases,
                "parent_id": upsert.anchor.parent_id,
                "depends_on": upsert.anchor.depends_on,
                "created_at_ms": upsert.anchor.created_at_ms,
                "updated_at_ms": upsert.anchor.updated_at_ms,
                "created": upsert.created
            },
            "scope": {
                "branch": commit_branch,
                "trace_doc": commit_trace_doc,
                "graph_doc": commit_graph_doc
            },
            "note": {
                "card_id": card_id,
                "inserted": commit.inserted,
                "nodes_upserted": commit.nodes_upserted,
                "edges_upserted": commit.edges_upserted,
                "last_seq": commit.last_seq
            }
        });

        if warnings.is_empty() {
            ai_ok("macro_anchor_note", result)
        } else {
            ai_ok_with_warnings("macro_anchor_note", result, warnings, Vec::new())
        }
    }
}
