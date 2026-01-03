#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_graph_merge(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let from = match require_string(args_obj, "from") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let into_opt = match optional_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string()),
            Err(resp) => return resp,
        };
        let cursor = match optional_i64(args_obj, "cursor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(200),
            Err(resp) => return resp,
        };
        let dry_run = match optional_bool(args_obj, "dry_run") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let merge_to_base = match optional_bool(args_obj, "merge_to_base") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let from_exists = match self.store.branch_exists(&workspace, &from) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !from_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown from-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }
        let into = if merge_to_base {
            let base = match self.store.branch_base_info(&workspace, &from) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(StoreError::UnknownBranch) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown from-branch",
                        Some("Call branch_list to discover existing branches, then retry."),
                        vec![suggest_call(
                            "branch_list",
                            "List known branches for this workspace.",
                            "high",
                            json!({ "workspace": workspace.as_str() }),
                        )],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some((base_branch, _base_seq)) = base else {
                return ai_error_with(
                    "MERGE_NOT_SUPPORTED",
                    "Merge not supported",
                    Some("merge_to_base requires from.branch_base to be set"),
                    vec![],
                );
            };
            if into_opt.as_ref().is_some_and(|into| into != &base_branch) {
                return ai_error(
                    "INVALID_INPUT",
                    "into: expected base branch for merge_to_base; fix: omit into or set into to the base branch",
                );
            }
            base_branch
        } else {
            match into_opt {
                Some(into) => into,
                None => {
                    return ai_error(
                        "INVALID_INPUT",
                        "into: expected branch name; fix: into=\"main\"",
                    );
                }
            }
        };

        let into_exists = match self.store.branch_exists(&workspace, &into) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !into_exists {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown into-branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            );
        }

        let merged = match self.store.graph_merge_back(
            &workspace,
            bm_storage::GraphMergeBackRequest {
                from_branch: from.clone(),
                into_branch: into.clone(),
                doc: doc.clone(),
                cursor,
                limit,
                dry_run,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::MergeNotSupported) => {
                return ai_error_with(
                    "MERGE_NOT_SUPPORTED",
                    "Merge not supported",
                    Some("Use merge_to_base=true or set into to the base branch."),
                    vec![],
                );
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
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let conflicts = merged
            .conflicts
            .iter()
            .map(Self::conflict_detail_to_json)
            .collect::<Vec<_>>();

        let mut suggestions = Vec::new();
        if !dry_run && let Some(conflict_id) = merged.conflict_ids.first() {
            suggestions.push(suggest_call(
                "graph_conflict_show",
                "Inspect the first merge conflict.",
                "high",
                json!({ "workspace": workspace.as_str(), "conflict_id": conflict_id }),
            ));
            suggestions.push(suggest_call(
                "graph_conflict_resolve",
                "Resolve the first conflict (pick ours/theirs).",
                "medium",
                json!({ "workspace": workspace.as_str(), "conflict_id": conflict_id, "resolution": "ours" }),
            ));
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "from": from,
            "into": into,
            "doc": doc,
            "merged": merged.merged,
            "skipped": merged.skipped,
            "conflicts_created": merged.conflicts_created,
            "conflict_ids": merged.conflict_ids,
            "conflicts": conflicts,
            "diff_summary": {
                "nodes_changed": merged.diff_summary.nodes_changed,
                "edges_changed": merged.diff_summary.edges_changed,
                "node_fields_changed": merged.diff_summary.node_fields_changed,
                "edge_fields_changed": merged.diff_summary.edge_fields_changed
            },
            "pagination": {
                "cursor": cursor,
                "next_cursor": merged.next_cursor,
                "has_more": merged.has_more,
                "limit": limit,
                "count": merged.count
            }
        });

        if suggestions.is_empty() {
            ai_ok("graph_merge", result)
        } else {
            ai_ok_with("graph_merge", result, suggestions)
        }
    }
}
