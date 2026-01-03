#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_tag_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let name = match require_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from = match optional_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let force = match optional_bool(args_obj, "force") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let default_branch = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let mut branch = default_branch.clone();
        let mut doc = DEFAULT_NOTES_DOC.to_string();

        let seq = match from {
            Some(raw) => {
                if let Some(seq) = parse_seq_reference(raw.trim()) {
                    seq
                } else if let Ok(Some(tag)) = self.store.vcs_tag_get(&workspace, raw.trim()) {
                    branch = tag.branch;
                    doc = tag.doc;
                    tag.seq
                } else if self
                    .store
                    .branch_exists(&workspace, raw.trim())
                    .unwrap_or(false)
                {
                    branch = raw.trim().to_string();
                    match self
                        .store
                        .doc_head_seq_for_branch_doc(&workspace, &branch, &doc)
                    {
                        Ok(Some(seq)) => seq,
                        Ok(None) => {
                            return ai_error("INVALID_INPUT", "no commits for branch");
                        }
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
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    }
                } else {
                    return ai_error("UNKNOWN_ID", "Unknown ref");
                }
            }
            None => match self
                .store
                .doc_head_seq_for_branch_doc(&workspace, &branch, &doc)
            {
                Ok(Some(seq)) => seq,
                Ok(None) => return ai_error("INVALID_INPUT", "no commits for branch"),
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
        };

        let tag = match self
            .store
            .vcs_tag_create(&workspace, &name, &branch, &doc, seq, force)
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
        };

        ai_ok(
            "tag_create",
            json!({
                "workspace": workspace.as_str(),
                "tag": {
                    "name": tag.name,
                    "branch": tag.branch,
                    "doc": tag.doc,
                    "seq": tag.seq,
                    "created_at_ms": tag.created_at_ms
                }
            }),
        )
    }

    pub(crate) fn tool_branchmind_tag_list(&mut self, args: Value) -> Value {
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

        let tags = match self.store.vcs_tag_list(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let tags_json = tags
            .into_iter()
            .map(|tag| {
                json!({
                    "name": tag.name,
                    "branch": tag.branch,
                    "doc": tag.doc,
                    "seq": tag.seq,
                    "created_at_ms": tag.created_at_ms
                })
            })
            .collect::<Vec<_>>();
        let tags_count = tags_json.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "tags": tags_json,
            "count": tags_count,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "tags", limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "tags", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("tag_list", result)
    }

    pub(crate) fn tool_branchmind_tag_delete(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let name = match require_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let deleted = match self.store.vcs_tag_delete(&workspace, &name) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        ai_ok(
            "tag_delete",
            json!({
                "workspace": workspace.as_str(),
                "name": name,
                "deleted": deleted
            }),
        )
    }
}
