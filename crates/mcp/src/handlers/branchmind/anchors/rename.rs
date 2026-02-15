#![forbid(unsafe_code)]

use super::{ANCHORS_GRAPH_DOC, ANCHORS_TRACE_DOC};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_rename(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let from_raw = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to_raw = match require_string(args_obj, "to") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let mut warnings = Vec::<Value>::new();

        // If caller passed an alias id instead of a canonical anchor id, resolve it to the
        // canonical id and proceed. This keeps refactors ergonomic while preserving a stable map.
        let mut effective_from = from_raw.clone();
        let existing = match self.store.anchor_get(
            &workspace,
            bm_storage::AnchorGetRequest {
                id: effective_from.clone(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if existing.is_none() {
            let resolved = match self
                .store
                .anchor_resolve_id(&workspace, effective_from.as_str())
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            if let Some(canonical) = resolved {
                warnings.push(warning(
                    "ANCHOR_ALIAS_RESOLVED",
                    "anchor id resolved via alias mapping",
                    "Using canonical id for refactor; history tagged with aliases stays discoverable automatically.",
                ));
                effective_from = canonical;
            }
        }

        let renamed = match self.store.anchor_rename(
            &workspace,
            bm_storage::AnchorRenameRequest {
                from_id: effective_from.clone(),
                to_id: to_raw.clone(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let from_id = renamed.from_id.clone();
        let to_id = renamed.to_id.clone();

        // Record a compact “map event” into the anchor graph so the refactor is part of the
        // durable story (helps long-gap resumes).
        let checkout = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut meta = serde_json::Map::new();
        meta.insert("anchor".to_string(), Value::String(to_id.clone()));
        meta.insert(
            "rename".to_string(),
            json!({ "from": &from_id, "to": &to_id }),
        );
        let card_value = json!({
            "type": "update",
            "title": "Anchor renamed",
            "text": format!("Renamed anchor id: {} → {}", from_id, to_id),
            "status": "open",
            "tags": [to_id.clone(), VIS_TAG_CANON],
            "meta": Value::Object(meta)
        });
        let parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let supports: Vec<String> = Vec::new();
        let blocks: Vec<String> = Vec::new();
        if let Err(resp) =
            self.commit_think_card_internal(super::super::graph::ThinkCardCommitInternalArgs {
                workspace: &workspace,
                branch: &checkout,
                trace_doc: ANCHORS_TRACE_DOC,
                graph_doc: ANCHORS_GRAPH_DOC,
                parsed,
                supports: &supports,
                blocks: &blocks,
            })
        {
            return resp;
        }

        let anchor = renamed.anchor;
        let bindings = match self
            .store
            .anchor_bindings_list_for_anchor(&workspace, anchor.id.as_str())
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let bindings_json = bindings
            .into_iter()
            .map(|b| {
                json!({
                    "kind": b.kind,
                    "repo_rel": b.repo_rel,
                    "created_at_ms": b.created_at_ms,
                    "updated_at_ms": b.updated_at_ms
                })
            })
            .collect::<Vec<_>>();

        let result = json!({
            "workspace": workspace.as_str(),
            "from": renamed.from_id,
            "to": renamed.to_id,
            "anchor": {
                "id": anchor.id,
                "title": anchor.title,
                "kind": anchor.kind,
                "status": anchor.status,
                "description": anchor.description,
                "refs": anchor.refs,
                "bindings": bindings_json,
                "aliases": anchor.aliases,
                "parent_id": anchor.parent_id,
                "depends_on": anchor.depends_on,
                "created_at_ms": anchor.created_at_ms,
                "updated_at_ms": anchor.updated_at_ms
            }
        });

        if warnings.is_empty() {
            ai_ok("anchors_rename", result)
        } else {
            ai_ok_with_warnings("anchors_rename", result, warnings, Vec::new())
        }
    }
}
