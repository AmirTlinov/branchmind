#![forbid(unsafe_code)]

use super::super::super::graph::ThinkCardCommitInternalArgs;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_publish(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let card_id = match require_string(args_obj, "card_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let pin = args_obj
            .get("pin")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let (branch, graph_doc, trace_doc) =
            match self.resolve_think_watch_scope(&workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let query = bm_storage::GraphQueryRequest {
            ids: Some(vec![card_id.clone()]),
            types: None,
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: 1,
            include_edges: false,
            edges_limit: 0,
        };
        let slice = match self
            .store
            .graph_query(&workspace, &branch, &graph_doc, query)
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
        let Some(source) = slice.nodes.into_iter().next() else {
            return ai_error("UNKNOWN_ID", "Unknown card id");
        };

        let published_id = format!("CARD-PUB-{}", card_id.trim());
        if published_id.len() > 1024 {
            return ai_error("INVALID_INPUT", "published id is too long");
        }

        let mut tags = source.tags.clone();
        // Rewrite lane tags and force the published copy into the shared lane.
        apply_lane_stamp_to_tags(&mut tags, None);
        // Publishing is a promotion into canon: never keep draft markers on the published copy.
        tags.retain(|t| !t.trim().eq_ignore_ascii_case(VIS_TAG_DRAFT));
        if !tags
            .iter()
            .any(|t| t.trim().eq_ignore_ascii_case(VIS_TAG_CANON))
        {
            tags.push(VIS_TAG_CANON.to_string());
        }
        if pin {
            tags.push(PIN_TAG.to_string());
            // Normalize (dedupe + lowercase).
            tags = bm_core::graph::normalize_tags(&tags)
                .unwrap_or_else(|_| vec![LANE_TAG_SHARED.to_string(), PIN_TAG.to_string()]);
        }

        let base_meta = source
            .meta_json
            .as_ref()
            .map(|raw| parse_json_or_string(raw));
        let meta_json = merge_meta_with_fields(
            base_meta,
            vec![
                (
                    "published_from".to_string(),
                    json!({
                        "card_id": card_id,
                        "agent_id": agent_id,
                    }),
                ),
                ("lane".to_string(), lane_meta_value(None)),
            ],
        );
        let mut meta_value = meta_json
            .as_ref()
            .map(|raw| parse_json_or_string(raw))
            .unwrap_or_else(|| json!({}));

        // Ensure meta.lane is present even if the base meta was not an object.
        apply_lane_stamp_to_meta(&mut meta_value, None);

        let parsed = ParsedThinkCard {
            card_id: Some(published_id.clone()),
            card_type: source.node_type.clone(),
            title: source.title.clone(),
            text: source.text.clone(),
            status: source.status.unwrap_or_else(|| "open".to_string()),
            tags,
            meta_value,
        };

        let supports = vec![source.id.clone()];
        let (card_id, commit) = match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
            workspace: &workspace,
            branch: &branch,
            trace_doc: &trace_doc,
            graph_doc: &graph_doc,
            parsed,
            supports: &supports,
            blocks: &[],
        }) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        ai_ok(
            "think_publish",
            json!({
                "workspace": workspace.as_str(),
                "branch": branch,
                "graph_doc": graph_doc,
                "trace_doc": trace_doc,
                "source_card_id": source.id,
                "published_card_id": card_id,
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
