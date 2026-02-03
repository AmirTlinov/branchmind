#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

use super::queries::graph_query_or_empty;

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperSignals {
    pub(super) blockers: Vec<Value>,
    pub(super) decisions: Vec<Value>,
    pub(super) evidence: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperSignalsLoadArgs {
    pub(super) decisions_limit: usize,
    pub(super) evidence_limit: usize,
    pub(super) blockers_limit: usize,
    pub(super) agent_id: Option<String>,
    pub(super) all_lanes: bool,
    pub(super) read_only: bool,
}

pub(super) fn load_resume_super_signals(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    reasoning: &bm_storage::ReasoningRefRow,
    args: ResumeSuperSignalsLoadArgs,
    reasoning_branch_missing: &mut bool,
) -> Result<ResumeSuperSignals, Value> {
    let ResumeSuperSignalsLoadArgs {
        decisions_limit,
        evidence_limit,
        blockers_limit,
        agent_id: _agent_id,
        all_lanes,
        read_only,
    } = args;
    let include_drafts = all_lanes;

    let mut decisions = Vec::new();
    if decisions_limit > 0 {
        let slice = graph_query_or_empty(
            server,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["decision".to_string()]),
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: decisions_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
            reasoning_branch_missing,
        )?;
        let mut filtered = slice
            .nodes
            .into_iter()
            .filter(|n| tags_visibility_allows(&n.tags, include_drafts, None))
            .collect::<Vec<_>>();
        filtered.truncate(decisions_limit);
        decisions = graph_nodes_to_signal_cards(filtered);
    }

    let mut evidence = Vec::new();
    if evidence_limit > 0 {
        let slice = graph_query_or_empty(
            server,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["evidence".to_string()]),
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: evidence_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
            reasoning_branch_missing,
        )?;
        let mut filtered = slice
            .nodes
            .into_iter()
            .filter(|n| tags_visibility_allows(&n.tags, include_drafts, None))
            .collect::<Vec<_>>();
        filtered.truncate(evidence_limit);
        evidence = graph_nodes_to_signal_cards(filtered);
    }

    let mut blockers = Vec::new();
    if blockers_limit > 0 {
        let slice = graph_query_or_empty(
            server,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: None,
                status: None,
                tags_any: Some(vec!["blocker".to_string()]),
                tags_all: None,
                text: None,
                cursor: None,
                limit: blockers_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
            reasoning_branch_missing,
        )?;
        let mut filtered = slice
            .nodes
            .into_iter()
            .filter(|n| tags_visibility_allows(&n.tags, include_drafts, None))
            .collect::<Vec<_>>();
        filtered.truncate(blockers_limit);
        blockers = graph_nodes_to_signal_cards(filtered);
    }

    Ok(ResumeSuperSignals {
        blockers,
        decisions,
        evidence,
    })
}
