#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_notes_commit(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let _agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let content = match require_string(args_obj, "content") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if content.trim().is_empty() {
            return ai_error("INVALID_INPUT", "content must not be empty");
        }

        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let target = if target.is_none() && branch.is_none() && doc.is_none() {
            match self.store.focus_get(&workspace) {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        } else {
            target
        };

        if target.is_some() && (branch.is_some() || doc.is_some()) {
            return ai_error(
                "INVALID_INPUT",
                "provide either target or (branch, doc), not both",
            );
        }

        let (branch, doc) = match target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return ai_error("INVALID_INPUT", "target must start with PLAN- or TASK-");
                    }
                };
                let reasoning = match self
                    .store
                    .ensure_reasoning_ref(&workspace, &target_id, kind)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => {
                        return ai_error("UNKNOWN_ID", "Unknown target id");
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (reasoning.branch, reasoning.notes_doc)
            }
            None => {
                let branch = match branch {
                    Some(branch) => branch,
                    None => match require_checkout_branch(&mut self.store, &workspace) {
                        Ok(branch) => branch,
                        Err(resp) => return resp,
                    },
                };
                let doc = doc.unwrap_or_else(|| DEFAULT_NOTES_DOC.to_string());
                (branch, doc)
            }
        };

        let title = match optional_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let format = match optional_string(args_obj, "format") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let base_meta = match optional_meta_value(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_json =
            merge_meta_with_fields(base_meta, vec![("lane".to_string(), lane_meta_value(None))]);

        let promote_to_knowledge = match optional_bool(args_obj, "promote_to_knowledge") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        if promote_to_knowledge && !self.note_promote_enabled {
            return ai_error_with(
                "FEATURE_DISABLED",
                "note promotion is disabled",
                Some("Enable via --note-promote (or env BRANCHMIND_NOTE_PROMOTE=1)."),
                Vec::new(),
            );
        }
        let knowledge_anchor = match optional_string(args_obj, "knowledge_anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let knowledge_key = match optional_string(args_obj, "knowledge_key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let knowledge_title = match optional_string(args_obj, "knowledge_title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let knowledge_key_mode = match optional_string(args_obj, "knowledge_key_mode") {
            Ok(v) => v.unwrap_or_else(|| "auto".to_string()),
            Err(resp) => return resp,
        };
        let knowledge_key_mode = knowledge_key_mode.trim().to_ascii_lowercase();
        if !matches!(knowledge_key_mode.as_str(), "explicit" | "auto") {
            return ai_error("INVALID_INPUT", "knowledge_key_mode must be explicit|auto");
        }

        let entry = match self.store.doc_append_note(
            &workspace,
            bm_storage::DocAppendRequest {
                branch,
                doc,
                title,
                format,
                meta_json,
                content,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut warnings = Vec::<Value>::new();
        let mut knowledge_result: Option<Value> = None;

        if promote_to_knowledge {
            let entry_content = entry.content.clone().unwrap_or_default();
            // Ensure KB branch exists before writing knowledge.
            let kb_branch = "kb/main";
            match self.store.branch_create(
                &workspace,
                kb_branch,
                Some(self.store.default_branch_name()),
            ) {
                Ok(_) | Err(StoreError::BranchAlreadyExists) => {}
                Err(err) => {
                    warnings.push(warning(
                        "KNOWLEDGE_PROMOTE_FAILED",
                        &format!("failed to create kb branch: {err}"),
                        "Knowledge promote skipped; retry later.",
                    ));
                }
            }

            if warnings.is_empty() {
                let mut card_obj = serde_json::Map::new();
                let card_title = knowledge_title
                    .clone()
                    .or_else(|| entry.title.clone())
                    .filter(|t| !t.trim().is_empty());
                if let Some(title) = card_title.as_deref() {
                    card_obj.insert("title".to_string(), Value::String(title.to_string()));
                }
                card_obj.insert("text".to_string(), Value::String(entry_content.clone()));
                card_obj.insert(
                    "tags".to_string(),
                    Value::Array(vec![Value::String(crate::VIS_TAG_DRAFT.to_string())]),
                );

                let mut knowledge_args = serde_json::Map::new();
                knowledge_args.insert(
                    "workspace".to_string(),
                    Value::String(workspace.as_str().to_string()),
                );
                knowledge_args.insert("branch".to_string(), Value::String(kb_branch.to_string()));
                knowledge_args.insert(
                    "graph_doc".to_string(),
                    Value::String("kb-graph".to_string()),
                );
                knowledge_args.insert(
                    "trace_doc".to_string(),
                    Value::String("kb-trace".to_string()),
                );
                if let Some(anchor) = knowledge_anchor.clone() {
                    knowledge_args.insert("anchor".to_string(), Value::String(anchor));
                }
                let mut key = knowledge_key.clone();
                if key.is_none() && knowledge_key_mode == "auto" {
                    let source = card_title.as_deref().unwrap_or(entry_content.as_str());
                    if let Some(slug) = crate::slugify_key(source) {
                        key = Some(slug);
                    } else {
                        warnings.push(warning(
                            "KNOWLEDGE_KEY_DERIVE_FAILED",
                            "auto key derivation failed",
                            "Provide knowledge_key explicitly or use a more specific title.",
                        ));
                    }
                }
                if let Some(key) = key {
                    knowledge_args.insert("key".to_string(), Value::String(key));
                }
                knowledge_args.insert(
                    "key_mode".to_string(),
                    Value::String(knowledge_key_mode.clone()),
                );
                knowledge_args.insert("card".to_string(), Value::Object(card_obj));

                let knowledge_resp =
                    self.tool_branchmind_think_add_knowledge(Value::Object(knowledge_args));
                if knowledge_resp
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    knowledge_result = knowledge_resp.get("result").cloned();
                    if let Some(w) = knowledge_resp.get("warnings").and_then(|v| v.as_array()) {
                        warnings.extend(w.clone());
                    }
                } else {
                    warnings.push(warning(
                        "KNOWLEDGE_PROMOTE_FAILED",
                        "knowledge promotion failed",
                        "Retry via think.knowledge.upsert or think.note.promote.",
                    ));
                }
            }
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "entry": {
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
            }
        });
        if let Some(knowledge_result) = knowledge_result
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("knowledge".to_string(), knowledge_result);
        }

        redact_value(&mut result, 6);
        if warnings.is_empty() {
            ai_ok("notes_commit", result)
        } else {
            ai_ok_with_warnings("notes_commit", result, warnings, Vec::new())
        }
    }
}
