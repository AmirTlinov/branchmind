#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct CandidateSlice {
    pub(super) cards: Vec<Value>,
    pub(super) edges: Vec<Value>,
}

pub(super) fn fetch(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    graph_doc: &str,
    limit_candidates: usize,
    warm_archive: bool,
    step_tag: Option<&str>,
) -> Result<CandidateSlice, Value> {
    if limit_candidates == 0 {
        return Ok(CandidateSlice {
            cards: Vec::new(),
            edges: Vec::new(),
        });
    }

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let recent_types = types
        .iter()
        .filter(|t| !matches!(t.as_str(), "note"))
        .cloned()
        .collect::<Vec<_>>();

    let tags_all = step_tag.map(|t| vec![t.to_string()]);
    let pins_tags_all = Some(match step_tag {
        None => vec![PIN_TAG.to_string()],
        Some(step_tag) => vec![PIN_TAG.to_string(), step_tag.to_string()],
    });

    // Relevance-first candidate selection:
    // 1) pinned cards,
    // 2) open frontier (hypotheses/questions/subgoals/tests),
    // 3) recent cards (fill), then materialize as a connected subgraph.
    let mut ordered_ids = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    let pins_limit = limit_candidates.min(8);
    let pins_slice = match server.store.graph_query(
        workspace,
        branch,
        graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types.clone()),
            status: None,
            tags_any: None,
            tags_all: pins_tags_all,
            text: None,
            cursor: None,
            limit: pins_limit,
            include_edges: false,
            edges_limit: 0,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownBranch) => {
            return Err(ai_error_with(
                "UNKNOWN_ID",
                "Unknown branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ));
        }
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    for node in &pins_slice.nodes {
        if seen.len() >= limit_candidates {
            break;
        }
        if seen.insert(node.id.clone()) {
            ordered_ids.push(node.id.clone());
        }
    }

    let open_each_limit = limit_candidates.min(6).max(1);
    for req in [
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["hypothesis".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["question".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["question".to_string()]),
            status: Some("open".to_string()),
            tags_any: Some(vec!["subgoal".to_string()]),
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit: open_each_limit.min(4),
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["test".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
    ] {
        if seen.len() >= limit_candidates {
            break;
        }
        let slice = match server.store.graph_query(workspace, branch, graph_doc, req) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return Err(ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                ));
            }
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        for node in &slice.nodes {
            if seen.len() >= limit_candidates {
                break;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
            }
        }
    }

    if seen.len() < limit_candidates {
        let remaining = limit_candidates.saturating_sub(seen.len());
        let padding = remaining.min(8);
        let recent_limit = remaining.saturating_add(padding).max(1);
        let recent_status = if warm_archive {
            None
        } else {
            Some("open".to_string())
        };
        let recent_slice = match server.store.graph_query(
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(recent_types),
                status: recent_status,
                tags_any: None,
                tags_all,
                text: None,
                cursor: None,
                limit: recent_limit,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return Err(ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                ));
            }
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        for node in &recent_slice.nodes {
            if seen.len() >= limit_candidates {
                break;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
            }
        }
    }

    if ordered_ids.is_empty() {
        return Ok(CandidateSlice {
            cards: Vec::new(),
            edges: Vec::new(),
        });
    }

    let graph_slice = match server.store.graph_query(
        workspace,
        branch,
        graph_doc,
        bm_storage::GraphQueryRequest {
            ids: Some(ordered_ids.clone()),
            types: None,
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: ordered_ids.len().max(1),
            include_edges: true,
            edges_limit: (ordered_ids.len().saturating_mul(6)).min(200),
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownBranch) => {
            return Err(ai_error_with(
                "UNKNOWN_ID",
                "Unknown branch",
                Some("Call branch_list to discover existing branches, then retry."),
                vec![suggest_call(
                    "branch_list",
                    "List known branches for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ));
        }
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    Ok(CandidateSlice {
        cards: reorder_cards_by_id(graph_nodes_to_cards(graph_slice.nodes), &ordered_ids),
        edges: graph_edges_to_json(graph_slice.edges),
    })
}

fn reorder_cards_by_id(mut cards: Vec<Value>, ordered_ids: &[String]) -> Vec<Value> {
    let mut by_id = std::collections::BTreeMap::<String, Value>::new();
    for card in cards.drain(..) {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        by_id.insert(id.to_string(), card);
    }

    let mut out = Vec::new();
    for id in ordered_ids {
        if let Some(card) = by_id.remove(id) {
            out.push(card);
        }
    }
    out.extend(by_id.into_values());
    out
}
