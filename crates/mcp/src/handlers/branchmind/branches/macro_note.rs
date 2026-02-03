#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_macro_branch_note(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let name = match optional_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from = match optional_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_NOTES_DOC.to_string()),
            Err(resp) => return resp,
        };
        if let Err(resp) = ensure_nonempty_doc(&Some(doc.clone()), "doc") {
            return resp;
        }
        let template = match optional_string(args_obj, "template") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let goal = match optional_string(args_obj, "goal") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let content = match optional_string(args_obj, "content") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if content.is_some() && template.is_some() {
            return ai_error("INVALID_INPUT", "provide content or template, not both");
        }
        let content = match (content, template) {
            (Some(content), None) => {
                if content.trim().is_empty() {
                    return ai_error("INVALID_INPUT", "content must not be empty");
                }
                content
            }
            (None, Some(template_id)) => {
                match render_note_template(&template_id, goal.as_deref()) {
                    Some(v) => v,
                    None => {
                        return ai_error(
                            "INVALID_INPUT",
                            "template: unknown id; valid: initiative|decision",
                        );
                    }
                }
            }
            (None, None) => return ai_error("INVALID_INPUT", "provide content or template"),
            (Some(_), Some(_)) => return ai_error("INVALID_INPUT", "provide content or template"),
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

        let omit_workspace = self.default_workspace.as_deref() == Some(workspace.as_str());
        let status_suggestion_params = {
            let mut obj = serde_json::Map::new();
            if !omit_workspace {
                obj.insert(
                    "workspace".to_string(),
                    Value::String(workspace.as_str().to_string()),
                );
            }
            Value::Object(obj)
        };

        let mut retry_note_params = serde_json::Map::new();
        if !omit_workspace {
            retry_note_params.insert(
                "workspace".to_string(),
                Value::String(workspace.as_str().to_string()),
            );
        }
        retry_note_params.insert("content".to_string(), Value::String(content.clone()));
        if let Some(agent_id) = agent_id.as_deref() {
            retry_note_params.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
        }
        if args_obj.contains_key("doc") {
            retry_note_params.insert("doc".to_string(), Value::String(doc.clone()));
        }
        if let Some(title) = title.as_ref() {
            retry_note_params.insert("title".to_string(), Value::String(title.clone()));
        }
        if let Some(format) = format.as_ref() {
            retry_note_params.insert("format".to_string(), Value::String(format.clone()));
        }
        if let Some(meta) = meta_json.as_ref()
            && let Ok(parsed) = serde_json::from_str::<Value>(meta)
        {
            retry_note_params.insert("meta".to_string(), parsed);
        }

        let (info, previous, current, branch_created) = if let Some(name) = name.clone() {
            let mut branch_created = false;
            let info = match self.store.branch_create(&workspace, &name, from.as_deref()) {
                Ok(v) => {
                    branch_created = true;
                    v
                }
                Err(StoreError::UnknownBranch) => {
                    let checkout = match self.store.branch_checkout_get(&workspace) {
                        Ok(v) => v,
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    let base = from.as_deref().unwrap_or("<checkout>");
                    let msg = format!("Unknown base branch: from=\"{base}\"");
                    let recovery = if let Some(checkout) = checkout.as_deref() {
                        format!(
                            "Omit from to base on checkout=\"{checkout}\", or choose an existing base branch."
                        )
                    } else {
                        "Call status to see defaults, then set checkout or choose a base branch."
                            .to_string()
                    };

                    let mut suggestions = Vec::new();
                    if checkout.is_some() {
                        let mut retry_params = retry_note_params.clone();
                        retry_params.insert("name".to_string(), Value::String(name.clone()));
                        suggestions.push(suggest_call(
                            "macro_branch_note",
                            "Retry by basing the new branch on the current checkout (omit from).",
                            "high",
                            Value::Object(retry_params),
                        ));
                    }
                    suggestions.push(suggest_call(
                        "status",
                        "Show checkout and defaults for this workspace.",
                        "medium",
                        status_suggestion_params.clone(),
                    ));

                    return ai_error_with("UNKNOWN_ID", &msg, Some(&recovery), suggestions);
                }
                Err(StoreError::BranchAlreadyExists) => {
                    // Daily portal should be resilient to retries and "checkout by name" usage.
                    // When the branch already exists, treat `name` as a checkout target (not a hard conflict)
                    // and keep `branch.created=false` so agents can tell no new branch was created.
                    let base_info = match self.store.branch_base_info(&workspace, &name) {
                        Ok(v) => v,
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    bm_storage::BranchInfo {
                        name: name.clone(),
                        base_branch: base_info.as_ref().map(|(base, _)| base.to_string()),
                        base_seq: base_info.map(|(_, seq)| seq),
                        created_at_ms: None,
                    }
                }
                Err(StoreError::BranchCycle) => {
                    return ai_error("INVALID_INPUT", "Branch base cycle");
                }
                Err(StoreError::BranchDepthExceeded) => {
                    return ai_error("INVALID_INPUT", "Branch base depth exceeded");
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let (previous, current) = match self.store.branch_checkout_set(&workspace, &name) {
                Ok(v) => v,
                Err(StoreError::UnknownBranch) => {
                    let checkout = match self.store.branch_checkout_get(&workspace) {
                        Ok(v) => v,
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    let msg = "Checkout failed after branch creation (unknown branch)".to_string();
                    let recovery = if let Some(checkout) = checkout.as_deref() {
                        format!("Current checkout=\"{checkout}\". Call status and retry.")
                    } else {
                        "Call status and retry.".to_string()
                    };
                    return ai_error_with(
                        "UNKNOWN_ID",
                        &msg,
                        Some(&recovery),
                        vec![suggest_call(
                            "status",
                            "Show checkout and defaults for this workspace.",
                            "high",
                            status_suggestion_params.clone(),
                        )],
                    );
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            (info, previous, current, branch_created)
        } else {
            // Note-only mode: append to checkout (or switch to an existing branch via `from`).
            let (previous, current) = if let Some(from) = from.as_ref() {
                match self.store.branch_checkout_set(&workspace, from) {
                    Ok(v) => v,
                    Err(StoreError::UnknownBranch) => {
                        let checkout = match self.store.branch_checkout_get(&workspace) {
                            Ok(v) => v,
                            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                        };
                        let msg = format!("Unknown branch: from=\"{from}\"");
                        let recovery = if let Some(checkout) = checkout.as_deref() {
                            format!(
                                "Omit from to write on checkout=\"{checkout}\", or create the branch explicitly (name=\"{from}\")."
                            )
                        } else {
                            "Call status to see defaults, then set checkout or create a branch."
                                .to_string()
                        };

                        // Suggest the safest no-side-effect recovery first (write on checkout).
                        let mut suggestions = Vec::new();
                        suggestions.push(suggest_call(
                            "macro_branch_note",
                            "Retry by writing the note on the current checkout branch (omit from).",
                            "high",
                            Value::Object(retry_note_params.clone()),
                        ));
                        let mut create_branch_params = retry_note_params.clone();
                        create_branch_params
                            .insert("name".to_string(), Value::String(from.to_string()));

                        suggestions.push(suggest_call(
                            "macro_branch_note",
                            "Create the missing branch (name=from) and write the note there.",
                            "medium",
                            Value::Object(create_branch_params),
                        ));

                        return ai_error_with("UNKNOWN_ID", &msg, Some(&recovery), suggestions);
                    }
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            } else {
                let current = match self.store.branch_checkout_get(&workspace) {
                    Ok(Some(v)) => v,
                    Ok(None) => {
                        return ai_error_with(
                            "NO_CHECKOUT",
                            "No checkout branch is set for this workspace",
                            Some(
                                "Call status to see workspace defaults, then set checkout or create a branch.",
                            ),
                            vec![suggest_call(
                                "status",
                                "Show workspace checkout and defaults.",
                                "high",
                                status_suggestion_params.clone(),
                            )],
                        );
                    }
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                (None, current)
            };

            (
                bm_storage::BranchInfo {
                    name: current.clone(),
                    base_branch: None,
                    base_seq: None,
                    created_at_ms: None,
                },
                previous,
                current,
                false,
            )
        };

        let entry = match self.store.doc_append_note(
            &workspace,
            bm_storage::DocAppendRequest {
                branch: current.clone(),
                doc: doc.clone(),
                title,
                format,
                meta_json,
                content,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                let msg = "Checkout points at a missing branch".to_string();
                let recovery = "Call status and set a valid checkout.".to_string();
                return ai_error_with(
                    "UNKNOWN_ID",
                    &msg,
                    Some(&recovery),
                    vec![suggest_call(
                        "status",
                        "Show checkout and defaults for this workspace.",
                        "high",
                        status_suggestion_params,
                    )],
                );
            }
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "macro_branch_note",
            json!({
                "workspace": workspace.as_str(),
                "branch": {
                    "name": info.name,
                    "base_branch": info.base_branch,
                    "base_seq": info.base_seq,
                    "created": branch_created
                },
                "checkout": {
                    "previous": previous,
                    "current": current
                },
                "note": {
                    "doc": doc,
                    "seq": entry.seq,
                    "ts": ts_ms_to_rfc3339(entry.ts_ms),
                    "ts_ms": entry.ts_ms
                }
            }),
        )
    }
}
