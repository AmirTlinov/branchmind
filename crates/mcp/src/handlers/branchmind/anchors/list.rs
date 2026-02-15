#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
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

        let text = match optional_string(args_obj, "text") {
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

        let list = match self.store.anchors_list(
            &workspace,
            bm_storage::AnchorsListRequest {
                text,
                kind,
                status,
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let anchors = list.anchors;
        let binding_hits = match self.store.anchor_bindings_list_for_anchors_any(
            &workspace,
            anchors.iter().map(|a| a.id.clone()).collect(),
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let mut bindings_by_anchor = BTreeMap::<String, Vec<Value>>::new();
        for hit in binding_hits {
            bindings_by_anchor
                .entry(hit.anchor_id.clone())
                .or_default()
                .push(json!({
                    "kind": hit.kind,
                    "repo_rel": hit.repo_rel,
                    "created_at_ms": hit.created_at_ms,
                    "updated_at_ms": hit.updated_at_ms
                }));
        }

        let anchors_json = anchors
            .into_iter()
            .map(|a| {
                let bindings = bindings_by_anchor.remove(&a.id).unwrap_or_default();
                json!({
                    "id": a.id,
                    "title": a.title,
                    "kind": a.kind,
                    "status": a.status,
                    "description": a.description,
                    "refs": a.refs,
                    "bindings": bindings,
                    "aliases": a.aliases,
                    "parent_id": a.parent_id,
                    "depends_on": a.depends_on,
                    "created_at_ms": a.created_at_ms,
                    "updated_at_ms": a.updated_at_ms
                })
            })
            .collect::<Vec<_>>();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "anchors": anchors_json,
            "count": anchors_json.len(),
            "has_more": list.has_more,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) =
                enforce_graph_list_budget(&mut result, "anchors", limit);

            if let Some(obj) = result.as_object_mut()
                && let Some(anchors) = obj.get("anchors").and_then(|v| v.as_array())
            {
                obj.insert(
                    "count".to_string(),
                    Value::Number(serde_json::Number::from(anchors.len() as u64)),
                );
            }

            set_truncated_flag(&mut result, budget_truncated);
            let _used = attach_budget(&mut result, limit, budget_truncated);

            let warnings = budget_warnings(budget_truncated, false, clamped);
            if warnings.is_empty() {
                ai_ok("anchors_list", result)
            } else {
                ai_ok_with_warnings("anchors_list", result, warnings, Vec::new())
            }
        } else {
            ai_ok("anchors_list", result)
        }
    }
}
