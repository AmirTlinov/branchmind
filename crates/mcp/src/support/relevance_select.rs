#![forbid(unsafe_code)]

use crate::*;
use bm_storage::GraphNode;
use serde_json::{Value, json};

pub(crate) struct RelevanceFirstCards {
    pub(crate) cards: Vec<Value>,
}

pub(crate) struct RelevanceFirstCardsArgs<'a> {
    pub(crate) workspace: &'a WorkspaceId,
    pub(crate) branch: &'a str,
    pub(crate) graph_doc: &'a str,
    pub(crate) cards_limit: usize,
    pub(crate) focus_step_tag: Option<&'a str>,
    pub(crate) agent_id: Option<&'a str>,
    pub(crate) warm_archive: bool,
    pub(crate) all_lanes: bool,
    pub(crate) read_only: bool,
}

fn empty_slice() -> bm_storage::GraphQuerySlice {
    bm_storage::GraphQuerySlice {
        nodes: Vec::new(),
        edges: Vec::new(),
        next_cursor: None,
        has_more: false,
    }
}

fn graph_query_or_empty(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
    request: bm_storage::GraphQueryRequest,
    read_only: bool,
) -> Result<bm_storage::GraphQuerySlice, Value> {
    match server.store.graph_query(workspace, branch, doc, request) {
        Ok(v) => Ok(v),
        Err(StoreError::UnknownBranch) => {
            if read_only {
                Ok(empty_slice())
            } else {
                Err(unknown_branch_error(workspace))
            }
        }
        Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}

fn node_to_card(node: GraphNode) -> Value {
    json!({
        "id": node.id,
        "type": node.node_type,
        "title": node.title,
        "text": node.text,
        "status": node.status,
        "tags": node.tags,
        "meta": node.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
        "deleted": node.deleted,
        "last_seq": node.last_seq,
        "last_ts_ms": node.last_ts_ms
    })
}

pub(crate) fn fetch_relevance_first_cards(
    server: &mut McpServer,
    args: RelevanceFirstCardsArgs<'_>,
) -> Result<RelevanceFirstCards, Value> {
    let RelevanceFirstCardsArgs {
        workspace,
        branch,
        graph_doc,
        cards_limit,
        focus_step_tag,
        agent_id: _agent_id,
        warm_archive,
        all_lanes,
        read_only,
    } = args;
    if cards_limit == 0 {
        return Ok(RelevanceFirstCards { cards: Vec::new() });
    }

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let recent_types = types.clone();

    let focus_step_tag = focus_step_tag.map(str::trim).filter(|t| !t.is_empty());
    let tags_all = focus_step_tag.map(|t| vec![t.to_string()]);
    // Pinned cards are global anchors: they must remain visible even when a step focus tag is
    // present (e.g., a task cockpit seeded by tasks_macro_delegate is pinned but not step-scoped).
    let pins_tags_all = Some(vec![PIN_TAG.to_string()]);

    let include_drafts = all_lanes;

    // Relevance-first selection:
    // 1) pinned cards (anchors),
    // 2) open frontier (hypotheses/questions/subgoals/tests),
    // 3) step-scoped warm slice (if step focus is present),
    // 4) recent fill (cold archive by default unless warm_archive=true).
    let mut ordered_ids = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut nodes_by_id = std::collections::BTreeMap::<String, GraphNode>::new();

    let pins_limit = cards_limit.min(8);
    let pins_slice = graph_query_or_empty(
        server,
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
        read_only,
    )?;
    for node in pins_slice.nodes {
        if seen.len() >= cards_limit {
            break;
        }
        if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
            continue;
        }
        if seen.insert(node.id.clone()) {
            ordered_ids.push(node.id.clone());
            nodes_by_id.insert(node.id.clone(), node);
        }
    }

    let open_each_limit = cards_limit.clamp(1, 6);
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
        if seen.len() >= cards_limit {
            break;
        }
        let slice = graph_query_or_empty(server, workspace, branch, graph_doc, req, read_only)?;
        for node in slice.nodes {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
                nodes_by_id.insert(node.id.clone(), node);
            }
        }
    }

    if focus_step_tag.is_some() && seen.len() < cards_limit {
        let step_any_limit = cards_limit.clamp(1, 4);
        let step_any_slice = graph_query_or_empty(
            server,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(recent_types.clone()),
                status: None,
                tags_any: None,
                tags_all: tags_all.clone(),
                text: None,
                cursor: None,
                limit: step_any_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
        )?;
        for node in step_any_slice.nodes {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
                nodes_by_id.insert(node.id.clone(), node);
            }
        }
    }

    // Fill remaining capacity with recent cards.
    if seen.len() < cards_limit {
        let remaining = cards_limit.saturating_sub(seen.len());
        let padding = remaining.min(8);
        let recent_limit = remaining.saturating_add(padding).max(1);
        let recent_status = if warm_archive {
            None
        } else {
            Some("open".to_string())
        };
        let recent_slice = graph_query_or_empty(
            server,
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
            read_only,
        )?;
        for node in recent_slice.nodes {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if !seen.insert(node.id.clone()) {
                continue;
            }
            ordered_ids.push(node.id.clone());
            nodes_by_id.insert(node.id.clone(), node);
        }
    }

    if ordered_ids.is_empty() {
        return Ok(RelevanceFirstCards { cards: Vec::new() });
    }

    let mut cards = Vec::new();
    for id in ordered_ids {
        let Some(node) = nodes_by_id.remove(&id) else {
            continue;
        };
        cards.push(node_to_card(node));
    }

    Ok(RelevanceFirstCards { cards })
}
