#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

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

impl McpServer {
    pub(crate) fn tool_branchmind_knowledge_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let _workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let include_drafts = match optional_bool(args_obj, "include_drafts") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let all_lanes = match optional_bool(args_obj, "all_lanes") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let mut tags_all = match optional_string_values(args_obj, "tags_all") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let anchor = match optional_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Some(anchor) = anchor {
            let tag = match normalize_anchor_tag(&anchor) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            if !tags_all.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                tags_all.push(tag);
            }
        }

        let include_drafts = include_drafts || all_lanes;
        if tags_all.is_empty() && !include_drafts {
            tags_all.push(VIS_TAG_CANON.to_string());
        }

        let mut obj = args_obj.clone();
        obj.insert("types".to_string(), json!(["knowledge"]));
        if !tags_all.is_empty() {
            obj.insert(
                "tags_all".to_string(),
                Value::Array(tags_all.into_iter().map(Value::String).collect()),
            );
        }

        self.tool_branchmind_think_query(Value::Object(obj))
    }
}
