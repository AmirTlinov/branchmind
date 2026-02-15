#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_merge(&mut self, args: Value) -> Value {
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
        let into = match require_string(args_obj, "into") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc_kind = args_obj
            .get("doc_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("notes");
        if doc_kind != "notes" && doc_kind != "trace" && doc_kind != "plan_spec" {
            return ai_error(
                "INVALID_INPUT",
                "doc_kind must be 'notes', 'trace', or 'plan_spec'",
            );
        }

        let doc = match optional_string(args_obj, "doc") {
            Ok(Some(v)) => v,
            Ok(None) if doc_kind == "trace" => DEFAULT_TRACE_DOC.to_string(),
            Ok(None) if doc_kind == "plan_spec" => {
                return ai_error("INVALID_INPUT", "doc is required when doc_kind='plan_spec'");
            }
            Ok(None) => "notes".to_string(),
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

        if doc_kind == "plan_spec" {
            let from_latest = match super::plan_spec::load_latest_plan_spec(
                &mut self.store,
                &workspace,
                &from,
                &doc,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let into_latest = match super::plan_spec::load_latest_plan_spec(
                &mut self.store,
                &workspace,
                &into,
                &doc,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

            let Some((from_entry, from_value)) = from_latest else {
                return ai_ok(
                    "merge",
                    json!({
                        "workspace": workspace.as_str(),
                        "from": from,
                        "into": into,
                        "doc": doc,
                        "doc_kind": doc_kind,
                        "merged": 0,
                        "skipped": 0,
                        "pagination": {
                            "cursor": cursor,
                            "next_cursor": Value::Null,
                            "has_more": false,
                            "limit": limit,
                            "count": 0
                        },
                        "plan_spec_merge": {
                            "status": "missing_from",
                            "reason": "source branch has no plan_spec entry"
                        }
                    }),
                );
            };

            let same_as_into = into_latest
                .as_ref()
                .map(|(_, into_value)| into_value == &from_value)
                .unwrap_or(false);

            if same_as_into {
                return ai_ok(
                    "merge",
                    json!({
                        "workspace": workspace.as_str(),
                        "from": from,
                        "into": into,
                        "doc": doc,
                        "doc_kind": doc_kind,
                        "merged": 0,
                        "skipped": 1,
                        "pagination": {
                            "cursor": cursor,
                            "next_cursor": Value::Null,
                            "has_more": false,
                            "limit": limit,
                            "count": 1
                        },
                        "plan_spec_merge": {
                            "status": "already_identical",
                            "from_seq": from_entry.seq,
                            "into_seq": into_latest.as_ref().map(|(entry, _)| entry.seq)
                        }
                    }),
                );
            }

            if dry_run {
                return ai_ok(
                    "merge",
                    json!({
                        "workspace": workspace.as_str(),
                        "from": from,
                        "into": into,
                        "doc": doc,
                        "doc_kind": doc_kind,
                        "merged": 1,
                        "skipped": 0,
                        "pagination": {
                            "cursor": cursor,
                            "next_cursor": Value::Null,
                            "has_more": false,
                            "limit": limit,
                            "count": 1
                        },
                        "plan_spec_merge": {
                            "status": "would_merge",
                            "from_seq": from_entry.seq,
                            "into_seq": into_latest.as_ref().map(|(entry, _)| entry.seq)
                        }
                    }),
                );
            }

            let content = serde_json::to_string_pretty(&from_value).map_err(|_| {
                ai_error(
                    "STORE_ERROR",
                    "failed to serialize canonical plan_spec JSON for merge",
                )
            });
            let content = match content {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let meta_json = json!({
                "merge": {
                    "doc_kind": "plan_spec",
                    "from_branch": from,
                    "from_seq": from_entry.seq
                }
            })
            .to_string();

            let appended = self.store.doc_append_plan_spec(
                &workspace,
                bm_storage::DocAppendRequest {
                    branch: into.clone(),
                    doc: doc.clone(),
                    title: Some(format!("plan_spec merge from {from}")),
                    format: Some("plan_spec.v1".to_string()),
                    meta_json: Some(meta_json),
                    content,
                },
            );
            let appended = match appended {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            return ai_ok(
                "merge",
                json!({
                    "workspace": workspace.as_str(),
                    "from": from,
                    "into": into,
                    "doc": doc,
                    "doc_kind": doc_kind,
                    "merged": 1,
                    "skipped": 0,
                    "pagination": {
                        "cursor": cursor,
                        "next_cursor": Value::Null,
                        "has_more": false,
                        "limit": limit,
                        "count": 1
                    },
                    "plan_spec_merge": {
                        "status": "merged",
                        "from_seq": from_entry.seq,
                        "into_seq": appended.seq
                    }
                }),
            );
        }

        let merged = match self.store.doc_merge_notes(
            &workspace,
            bm_storage::DocMergeNotesRequest {
                from_branch: from.clone(),
                into_branch: into.clone(),
                doc: doc.clone(),
                cursor,
                limit,
                dry_run,
            },
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
            "merge",
            json!({
                "workspace": workspace.as_str(),
                "from": from,
                "into": into,
                "doc": doc,
                "doc_kind": doc_kind,
                "merged": merged.merged,
                "skipped": merged.skipped,
                "pagination": {
                    "cursor": cursor,
                    "next_cursor": merged.next_cursor,
                    "has_more": merged.has_more,
                    "limit": limit,
                    "count": merged.count
                }
            }),
        )
    }
}
