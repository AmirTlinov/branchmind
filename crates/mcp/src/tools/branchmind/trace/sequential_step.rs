#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_trace_sequential_step(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let thought = match require_string(args_obj, "thought") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if thought.trim().is_empty() {
            return ai_error("INVALID_INPUT", "thought must not be empty");
        }

        let thought_number = match optional_i64(args_obj, "thoughtNumber") {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("INVALID_INPUT", "thoughtNumber is required"),
            Err(resp) => return resp,
        };
        if thought_number <= 0 {
            return ai_error("INVALID_INPUT", "thoughtNumber must be positive");
        }
        let total_thoughts = match optional_i64(args_obj, "totalThoughts") {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("INVALID_INPUT", "totalThoughts is required"),
            Err(resp) => return resp,
        };
        if total_thoughts <= 0 || total_thoughts < thought_number {
            return ai_error(
                "INVALID_INPUT",
                "totalThoughts must be positive and >= thoughtNumber",
            );
        }
        let next_thought_needed = match optional_bool(args_obj, "nextThoughtNeeded") {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("INVALID_INPUT", "nextThoughtNeeded is required"),
            Err(resp) => return resp,
        };

        let message = match optional_string(args_obj, "message") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let confidence = match optional_string(args_obj, "confidence") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let goal = match optional_string(args_obj, "goal") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let needs_more_thoughts = match optional_string(args_obj, "needsMoreThoughts") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let branch_id = match optional_string(args_obj, "branchId") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };
        let branch_from_thought = match optional_i64(args_obj, "branchFromThought") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Some(value) = branch_from_thought
            && value <= 0
        {
            return ai_error("INVALID_INPUT", "branchFromThought must be positive");
        }
        let is_revision = match optional_bool(args_obj, "isRevision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let revises_thought = match optional_i64(args_obj, "revisesThought") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Some(value) = revises_thought
            && value <= 0
        {
            return ai_error("INVALID_INPUT", "revisesThought must be positive");
        }
        if is_revision.unwrap_or(false) && revises_thought.is_none() {
            return ai_error(
                "INVALID_INPUT",
                "revisesThought is required when isRevision is true",
            );
        }

        let meta_value = match optional_meta_value(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (branch, trace_doc) = match self.resolve_trace_scope(&workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut meta_fields = vec![
            (
                "thoughtNumber".to_string(),
                Value::Number(serde_json::Number::from(thought_number)),
            ),
            (
                "totalThoughts".to_string(),
                Value::Number(serde_json::Number::from(total_thoughts)),
            ),
            (
                "nextThoughtNeeded".to_string(),
                Value::Bool(next_thought_needed),
            ),
        ];

        if let Some(value) = is_revision {
            meta_fields.push(("isRevision".to_string(), Value::Bool(value)));
        }
        if let Some(value) = revises_thought {
            meta_fields.push((
                "revisesThought".to_string(),
                Value::Number(serde_json::Number::from(value)),
            ));
        }
        if let Some(value) = branch_from_thought {
            meta_fields.push((
                "branchFromThought".to_string(),
                Value::Number(serde_json::Number::from(value)),
            ));
        }
        if let Some(value) = branch_id {
            meta_fields.push(("branchId".to_string(), Value::String(value)));
        }
        if let Some(value) = needs_more_thoughts {
            meta_fields.push(("needsMoreThoughts".to_string(), Value::String(value)));
        }
        if let Some(value) = confidence {
            meta_fields.push(("confidence".to_string(), Value::String(value)));
        }
        if let Some(value) = goal {
            meta_fields.push(("goal".to_string(), Value::String(value)));
        }

        let meta_json = merge_meta_with_fields(meta_value, meta_fields);

        let entry = match self.store.doc_append_trace(
            &workspace,
            bm_storage::DocAppendRequest {
                branch: branch.clone(),
                doc: trace_doc.clone(),
                title: message.clone(),
                format: Some("trace_sequential_step".to_string()),
                meta_json: meta_json.clone(),
                content: thought,
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

        ai_ok(
            "trace_sequential_step",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "doc": trace_doc,
                "entry": entry_json
            }),
        )
    }
}
