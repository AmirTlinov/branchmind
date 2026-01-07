#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_trace_step(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let step = match require_string(args_obj, "step") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if step.trim().is_empty() {
            return ai_error("INVALID_INPUT", "step must not be empty");
        }

        let message = match optional_string(args_obj, "message") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let mode = match optional_string(args_obj, "mode") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let supports = match optional_string_or_string_array(args_obj, "supports") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let blocks = match optional_string_or_string_array(args_obj, "blocks") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let base = match optional_string(args_obj, "base") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let checkpoint_every = match optional_usize(args_obj, "checkpoint_every") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_value = match optional_meta_value(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_warnings = trace_step_sequential_meta_warnings(meta_value.as_ref());

        let (branch, trace_doc) = match self.resolve_trace_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut meta_fields = Vec::new();
        if let Some(mode) = mode {
            meta_fields.push(("mode".to_string(), Value::String(mode)));
        }
        if let Some(supports) = supports {
            meta_fields.push((
                "supports".to_string(),
                Value::Array(supports.into_iter().map(Value::String).collect()),
            ));
        }
        if let Some(blocks) = blocks {
            meta_fields.push((
                "blocks".to_string(),
                Value::Array(blocks.into_iter().map(Value::String).collect()),
            ));
        }
        if let Some(base) = base {
            meta_fields.push(("base".to_string(), Value::String(base)));
        }
        if let Some(checkpoint_every) = checkpoint_every {
            meta_fields.push((
                "checkpoint_every".to_string(),
                Value::Number(serde_json::Number::from(checkpoint_every as u64)),
            ));
        }
        let meta_json = merge_meta_with_fields(meta_value, meta_fields);

        let entry = match self.store.doc_append_trace(
            &workspace,
            bm_storage::DocAppendRequest {
                branch: branch.clone(),
                doc: trace_doc.clone(),
                title: message.clone(),
                format: Some("trace_step".to_string()),
                meta_json: meta_json.clone(),
                content: step,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entry_json = json!({
            "seq": entry.seq,
            "ts": ts_ms_to_rfc3339(entry.ts_ms),
            "ts_ms": entry.ts_ms,
            "branch": entry.branch,
            "doc": entry.doc,
            "kind": entry.kind.as_str(),
            "title": entry.title,
            "format": entry.format,
            "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
            "content": entry.content
        });

        let result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "doc": trace_doc,
            "entry": entry_json
        });

        if meta_warnings.is_empty() {
            ai_ok("trace_step", result)
        } else {
            ai_ok_with_warnings("trace_step", result, meta_warnings, Vec::new())
        }
    }
}
