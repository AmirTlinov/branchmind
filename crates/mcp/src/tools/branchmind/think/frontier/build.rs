#![forbid(unsafe_code)]

use super::query::graph_query_cards;
use super::{ThinkFrontier, ThinkFrontierLimits};
use crate::*;
use serde_json::Value;

impl McpServer {
    pub(in super::super) fn build_think_frontier(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        graph_doc: &str,
        limits: ThinkFrontierLimits,
        step_tag: Option<&str>,
    ) -> Result<ThinkFrontier, Value> {
        let tags_all = step_tag.map(|t| vec![t.to_string()]);
        let hypotheses = graph_query_cards(
            self,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["hypothesis".to_string()]),
                status: Some("open".to_string()),
                tags_any: None,
                tags_all: tags_all.clone(),
                text: None,
                cursor: None,
                limit: limits.hypotheses,
                include_edges: false,
                edges_limit: 0,
            },
        )?;

        let questions = graph_query_cards(
            self,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["question".to_string()]),
                status: Some("open".to_string()),
                tags_any: None,
                tags_all: tags_all.clone(),
                text: None,
                cursor: None,
                limit: limits.questions,
                include_edges: false,
                edges_limit: 0,
            },
        )?;

        let subgoals = graph_query_cards(
            self,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["question".to_string()]),
                status: Some("open".to_string()),
                tags_any: Some(vec!["subgoal".to_string()]),
                tags_all: tags_all.clone(),
                text: None,
                cursor: None,
                limit: limits.subgoals,
                include_edges: false,
                edges_limit: 0,
            },
        )?;

        let tests = graph_query_cards(
            self,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["test".to_string()]),
                status: Some("open".to_string()),
                tags_any: None,
                tags_all,
                text: None,
                cursor: None,
                limit: limits.tests,
                include_edges: false,
                edges_limit: 0,
            },
        )?;

        Ok(ThinkFrontier {
            hypotheses,
            questions,
            subgoals,
            tests,
        })
    }
}
