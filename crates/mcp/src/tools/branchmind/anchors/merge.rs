#![forbid(unsafe_code)]

use super::{ANCHORS_GRAPH_DOC, ANCHORS_TRACE_DOC};
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_merge(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let into_raw = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_raw = match optional_string_array(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(from_raw) = from_raw else {
            return ai_error("INVALID_INPUT", "from is required");
        };
        let from_raw = match normalize_required_string_list(from_raw, "from") {
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

        // Resolve canonical ids for better ergonomics: callers can pass alias ids in `into` or
        // `from` and still get the intended merge behavior.
        let into = match self.store.anchor_resolve_id(&workspace, &into_raw) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let Some(into) = into else {
            return ai_error("INVALID_INPUT", "into not found");
        };
        if !into.eq_ignore_ascii_case(&into_raw) {
            warnings.push(warning(
                "ANCHOR_ALIAS_RESOLVED",
                "anchor id resolved via alias mapping",
                "Using canonical into id for merge; alias-tagged history remains discoverable automatically.",
            ));
        }

        let mut from_ids = Vec::<String>::new();
        let mut skipped = Vec::<String>::new();
        for raw in from_raw {
            let resolved = match self.store.anchor_resolve_id(&workspace, &raw) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(resolved) = resolved else {
                return ai_error("INVALID_INPUT", "from contains an unknown anchor id");
            };
            if resolved.eq_ignore_ascii_case(&into) {
                skipped.push(resolved);
                continue;
            }
            if !resolved.eq_ignore_ascii_case(&raw) {
                warnings.push(warning(
                    "ANCHOR_ALIAS_RESOLVED",
                    "anchor id resolved via alias mapping",
                    "Using canonical from id for merge; aliases remain discoverable automatically.",
                ));
            }
            from_ids.push(resolved);
        }
        from_ids.sort();
        from_ids.dedup();
        if from_ids.is_empty() {
            return ai_error("INVALID_INPUT", "from must contain at least one anchor id");
        }

        let merged = match self.store.anchors_merge(
            &workspace,
            bm_storage::AnchorsMergeRequest {
                into_id: into.clone(),
                from_ids: from_ids.clone(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let into_id = merged.into_id.clone();
        let merged_ids = merged.merged_ids.clone();

        // Record a compact “map event” into the anchors graph so the merge remains part of the
        // durable story (helps long-gap resumes).
        let checkout = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut meta = serde_json::Map::new();
        meta.insert("anchor".to_string(), Value::String(into_id.clone()));
        meta.insert(
            "merge".to_string(),
            json!({ "into": &into_id, "from": &merged_ids }),
        );
        let card_value = json!({
            "type": "update",
            "title": "Anchors merged",
            "text": format!("Merged anchors into {}.", into_id),
            "status": "open",
            "tags": [into_id.clone(), VIS_TAG_CANON],
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

        let anchor = merged.anchor;
        let result = json!({
            "workspace": workspace.as_str(),
            "into": into_id,
            "from": from_ids,
            "merged": merged_ids,
            "skipped": skipped,
            "anchor": {
                "id": anchor.id,
                "title": anchor.title,
                "kind": anchor.kind,
                "status": anchor.status,
                "description": anchor.description,
                "refs": anchor.refs,
                "aliases": anchor.aliases,
                "parent_id": anchor.parent_id,
                "depends_on": anchor.depends_on,
                "created_at_ms": anchor.created_at_ms,
                "updated_at_ms": anchor.updated_at_ms
            }
        });

        // Merge is mutating but small; still keep warnings visible for alias-resolution cases.
        if warnings.is_empty() {
            ai_ok("anchors_merge", result)
        } else {
            // Keep warnings low-noise by de-duping (multiple from ids can resolve via alias).
            warnings.sort_by_key(|w| w.to_string());
            warnings.dedup_by(|a, b| a == b);
            ai_ok_with_warnings("anchors_merge", result, warnings, Vec::new())
        }
    }
}
