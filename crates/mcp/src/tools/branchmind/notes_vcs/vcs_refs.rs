#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_reflog(&mut self, args: Value) -> Value {
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
            Ok(v) => v.unwrap_or(50),
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
        let doc = DEFAULT_NOTES_DOC.to_string();

        if !self
            .store
            .branch_exists(&workspace, &ref_name)
            .unwrap_or(false)
            && self
                .store
                .vcs_tag_get(&workspace, &ref_name)
                .unwrap_or(None)
                .is_none()
            && self
                .store
                .vcs_ref_get(&workspace, &ref_name, &doc)
                .unwrap_or(None)
                .is_none()
        {
            return ai_error("UNKNOWN_ID", "Unknown ref");
        }

        let entries = match self
            .store
            .vcs_reflog_list(&workspace, &ref_name, &doc, limit)
        {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let entries_json = entries
            .into_iter()
            .map(|entry| {
                json!({
                    "ref": entry.reference,
                    "branch": entry.branch,
                    "doc": entry.doc,
                    "old_seq": entry.old_seq,
                    "new_seq": entry.new_seq,
                    "message": entry.message,
                    "ts": ts_ms_to_rfc3339(entry.ts_ms),
                    "ts_ms": entry.ts_ms
                })
            })
            .collect::<Vec<_>>();
        let entries_count = entries_json.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "ref": ref_name,
            "doc": doc,
            "entries": entries_json,
            "count": entries_count,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "entries", limit);
            set_truncated_flag(&mut result, truncated);
            let used = attach_budget(&mut result, limit, truncated);
            if used > limit {
                let (_used2, truncated2) = enforce_graph_list_budget(&mut result, "entries", limit);
                let truncated_final = truncated || truncated2;
                set_truncated_flag(&mut result, truncated_final);
                let _ = attach_budget(&mut result, limit, truncated_final);
            }
        }

        ai_ok("reflog", result)
    }

    pub(crate) fn tool_branchmind_reset(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let reference = match require_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let branch = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = DEFAULT_NOTES_DOC.to_string();

        let target_seq = if let Some(seq) = parse_seq_reference(reference.trim()) {
            seq
        } else if let Ok(Some(tag)) = self.store.vcs_tag_get(&workspace, reference.trim()) {
            tag.seq
        } else if self
            .store
            .branch_exists(&workspace, reference.trim())
            .unwrap_or(false)
        {
            match self
                .store
                .doc_head_seq_for_branch_doc(&workspace, reference.trim(), &doc)
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
            }
        } else {
            return ai_error("UNKNOWN_ID", "Unknown ref");
        };

        let visible = match self
            .store
            .doc_entry_visible(&workspace, &branch, &doc, target_seq)
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
        if !visible {
            return ai_error("INVALID_INPUT", "commit not visible for branch");
        }

        let update = match self.store.vcs_ref_set(
            &workspace,
            &branch,
            &branch,
            &doc,
            target_seq,
            Some(format!("reset:{reference}")),
        ) {
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
            "reset",
            json!({
                "workspace": workspace.as_str(),
                "ref": branch,
                "doc": doc,
                "old_seq": update.old_seq,
                "new_seq": update.reference.seq
            }),
        )
    }
}
