#![forbid(unsafe_code)]

use super::super::super::super::graph::ThinkCardCommitInternalArgs;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_subgoal_open(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let question_id = match require_string(args_obj, "question_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let reference = match optional_string(args_obj, "ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_graph_doc = match optional_string(args_obj, "parent_graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent_trace_doc = match optional_string(args_obj, "parent_trace_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let child_graph_doc = match optional_string(args_obj, "child_graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let child_trace_doc = match optional_string(args_obj, "child_trace_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let message = match optional_string(args_obj, "message") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let meta_json = match optional_object_as_json_string(args_obj, "meta") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let branch = match reference {
            Some(v) => v,
            None => match require_checkout_branch(&mut self.store, &workspace) {
                Ok(v) => v,
                Err(resp) => return resp,
            },
        };
        if !self
            .store
            .branch_exists(&workspace, &branch)
            .unwrap_or(false)
        {
            return unknown_branch_error(&workspace);
        }

        let parent_graph_doc = parent_graph_doc.unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
        let parent_trace_doc = parent_trace_doc.unwrap_or_else(|| DEFAULT_TRACE_DOC.to_string());

        let subgoal_id = match self.store.next_card_id(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let child_graph_doc = child_graph_doc.unwrap_or_else(|| format!("{subgoal_id}-graph"));
        let child_trace_doc = child_trace_doc.unwrap_or_else(|| format!("{subgoal_id}-trace"));

        let mut meta = serde_json::Map::new();
        meta.insert(
            "parent_question_id".to_string(),
            Value::String(question_id.clone()),
        );
        meta.insert(
            "child_graph_doc".to_string(),
            Value::String(child_graph_doc.clone()),
        );
        meta.insert(
            "child_trace_doc".to_string(),
            Value::String(child_trace_doc.clone()),
        );
        if let Some(raw) = meta_json
            && let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&raw)
        {
            for (k, v) in obj {
                meta.insert(k, v);
            }
        }

        let title = message.clone().unwrap_or_else(|| "Subgoal".to_string());
        let card_value = json!({
            "id": subgoal_id,
            "type": "question",
            "title": title,
            "status": "open",
            "tags": ["subgoal"],
            "meta": Value::Object(meta)
        });
        let parsed = match parse_think_card(&workspace, card_value) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let (card_id, commit) = match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
            workspace: &workspace,
            branch: &branch,
            trace_doc: &child_trace_doc,
            graph_doc: &parent_graph_doc,
            parsed,
            supports: &[],
            blocks: &[],
        }) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let edge_meta = merge_meta_with_message(None, message, None);
        if let Err(err) = self.store.graph_apply_ops(
            &workspace,
            &branch,
            &parent_graph_doc,
            vec![bm_storage::GraphOp::EdgeUpsert(
                bm_storage::GraphEdgeUpsert {
                    from: question_id.clone(),
                    rel: "subgoal".to_string(),
                    to: card_id.clone(),
                    meta_json: edge_meta,
                },
            )],
        ) {
            return match err {
                StoreError::UnknownBranch => unknown_branch_error(&workspace),
                StoreError::InvalidInput(msg) => ai_error("INVALID_INPUT", msg),
                err => ai_error("STORE_ERROR", &format_store_error(err)),
            };
        }

        ai_ok(
            "think_subgoal_open",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "parent_graph_doc": parent_graph_doc,
                "parent_trace_doc": parent_trace_doc,
                "child_graph_doc": child_graph_doc,
                "child_trace_doc": child_trace_doc,
                "subgoal_id": card_id,
                "inserted": commit.inserted,
                "graph_applied": {
                    "nodes_upserted": commit.nodes_upserted,
                    "edges_upserted": commit.edges_upserted
                },
                "last_seq": commit.last_seq
            }),
        )
    }
}
