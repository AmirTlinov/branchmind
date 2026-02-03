#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_commit(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let artifact = match require_string(args_obj, "artifact") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if artifact.trim().is_empty() {
            return ai_error("INVALID_INPUT", "artifact must not be empty");
        }
        let message = match require_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if message.trim().is_empty() {
            return ai_error("INVALID_INPUT", "message must not be empty");
        }

        let docs = match optional_string_or_string_array(args_obj, "docs") {
            Ok(v) => v.unwrap_or_else(|| vec![DEFAULT_NOTES_DOC.to_string()]),
            Err(resp) => return resp,
        };
        let meta_json = match optional_object_as_json_string(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let branch = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut commits = Vec::with_capacity(docs.len());
        for doc in &docs {
            let entry = match self.store.doc_append_note(
                &workspace,
                bm_storage::DocAppendRequest {
                    branch: branch.clone(),
                    doc: doc.clone(),
                    title: Some(message.clone()),
                    format: Some("commit".to_string()),
                    meta_json: meta_json.clone(),
                    content: artifact.clone(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            match self.store.vcs_ref_set(
                &workspace,
                &branch,
                &branch,
                doc,
                entry.seq,
                Some(message.clone()),
            ) {
                Ok(_) => {}
                Err(StoreError::UnknownBranch) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown branch",
                        Some("Call branch_list to discover existing branches, then retry."),
                        vec![suggest_call(
                            "branch_list",
                            "List known branches for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }

            commits.push(json!({
                "seq": entry.seq,
                "ts": ts_ms_to_rfc3339(entry.ts_ms),
                "ts_ms": entry.ts_ms,
                "branch": entry.branch,
                "doc": entry.doc,
                "message": entry.title,
                "format": entry.format,
                "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                "artifact": entry.content
            }));
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "branch": branch,
            "docs": docs,
            "commits": commits
        });
        redact_value(&mut result, 6);
        ai_ok("commit", result)
    }

    pub(crate) fn tool_branchmind_log(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let reference = match optional_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let ref_name = match reference {
            Some(v) => v,
            None => match require_checkout_branch(&mut self.store, &workspace) {
                Ok(v) => v,
                Err(resp) => return resp,
            },
        };
        let mut doc = DEFAULT_NOTES_DOC.to_string();

        let tag = match self.store.vcs_tag_get(&workspace, &ref_name) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let branch = if self
            .store
            .branch_exists(&workspace, &ref_name)
            .unwrap_or(false)
        {
            ref_name.clone()
        } else if let Some(tag) = tag.as_ref() {
            doc = tag.doc.clone();
            tag.branch.clone()
        } else {
            return ai_error("UNKNOWN_ID", "Unknown ref");
        };

        let head_seq = if let Some(tag) = tag {
            Some(tag.seq)
        } else {
            match self.store.vcs_ref_get(&workspace, &branch, &doc) {
                Ok(Some(v)) => Some(v.seq),
                Ok(None) => match self
                    .store
                    .doc_head_seq_for_branch_doc(&workspace, &branch, &doc)
                {
                    Ok(v) => v,
                    Err(StoreError::UnknownBranch) => {
                        return ai_error_with(
                            "UNKNOWN_ID",
                            "Unknown branch",
                            Some("Call branch_list to discover existing branches, then retry."),
                            vec![suggest_call(
                                "branch_list",
                                "List known branches for this workspace.",
                                "high",
                                json!({ "workspace": workspace.as_str() }),
                            )],
                        );
                    }
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                },
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };

        let cursor = head_seq.map(|seq| seq.saturating_add(1));
        let slice = match self
            .store
            .doc_show_tail(&workspace, &branch, &doc, cursor, limit)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let commits = slice
            .entries
            .into_iter()
            .filter(|entry| entry.kind == bm_storage::DocEntryKind::Note)
            .map(|entry| {
                json!({
                    "seq": entry.seq,
                    "ts": ts_ms_to_rfc3339(entry.ts_ms),
                    "ts_ms": entry.ts_ms,
                    "branch": entry.branch,
                    "doc": entry.doc,
                    "message": entry.title,
                    "format": entry.format,
                    "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "artifact": entry.content
                })
            })
            .collect::<Vec<_>>();
        let commit_count = commits.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "ref": ref_name,
            "branch": branch,
            "doc": doc,
            "head_seq": head_seq,
            "commits": commits,
            "pagination": {
                "cursor": cursor,
                "next_cursor": slice.next_cursor,
                "has_more": slice.has_more,
                "limit": limit,
                "count": commit_count
            },
            "truncated": false
        });

        redact_value(&mut result, 6);

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "commits", limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "commits", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("log", result)
    }
}
