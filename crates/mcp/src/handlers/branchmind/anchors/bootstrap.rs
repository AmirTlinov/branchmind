#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_bootstrap(&mut self, args: Value) -> Value {
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

        let Some(anchors_raw) = args_obj.get("anchors") else {
            return ai_error("INVALID_INPUT", "anchors is required");
        };
        let Some(anchors_arr) = anchors_raw.as_array() else {
            return ai_error("INVALID_INPUT", "anchors must be an array");
        };
        if anchors_arr.is_empty() {
            return ai_error("INVALID_INPUT", "anchors must not be empty");
        }

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let mut requests = Vec::<bm_storage::AnchorUpsertRequest>::with_capacity(anchors_arr.len());
        for (idx, item) in anchors_arr.iter().enumerate() {
            let Some(obj) = item.as_object() else {
                return ai_error(
                    "INVALID_INPUT",
                    &format!("anchors[{idx}] must be an object"),
                );
            };

            let id = match require_string(obj, "id") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let title = match require_string(obj, "title") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let kind = match require_string(obj, "kind") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let status = match optional_string(obj, "status") {
                Ok(v) => v.unwrap_or_else(|| "active".to_string()),
                Err(resp) => return resp,
            };
            let description = match optional_nullable_string(obj, "description") {
                Ok(v) => v.flatten().filter(|s| !s.trim().is_empty()),
                Err(resp) => return resp,
            };
            let refs = match optional_string_array(obj, "refs") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };
            let aliases = match optional_string_array(obj, "aliases") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };
            let parent_id = match optional_nullable_string(obj, "parent_id") {
                Ok(v) => v.flatten().filter(|s| !s.trim().is_empty()),
                Err(resp) => return resp,
            };
            let depends_on = match optional_string_array(obj, "depends_on") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };

            requests.push(bm_storage::AnchorUpsertRequest {
                id,
                title,
                kind,
                description,
                refs,
                aliases,
                parent_id,
                depends_on,
                status,
            });
        }

        let boot = match self.store.anchors_bootstrap(
            &workspace,
            bm_storage::AnchorsBootstrapRequest { anchors: requests },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut created = 0usize;
        let mut updated = 0usize;
        let mut anchors_json = Vec::<Value>::with_capacity(boot.anchors.len());
        for a in boot.anchors {
            if a.created {
                created += 1;
            } else {
                updated += 1;
            }
            anchors_json.push(json!({ "id": a.anchor.id, "created": a.created }));
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "anchors": anchors_json,
            "count": anchors_json.len(),
            "created": created,
            "updated": updated,
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
                ai_ok("anchors_bootstrap", result)
            } else {
                ai_ok_with_warnings("anchors_bootstrap", result, warnings, Vec::new())
            }
        } else {
            ai_ok("anchors_bootstrap", result)
        }
    }
}
