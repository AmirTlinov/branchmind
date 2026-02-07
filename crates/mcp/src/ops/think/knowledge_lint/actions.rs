#![forbid(unsafe_code)]

use super::model::{
    CrossDuplicateGroup, DuplicateGroup, OverloadedKeySummary, OverloadedOutliersGroup,
};
use crate::ops::{Action, ActionPriority, OpResponse, ToolName};
use serde_json::json;

pub(crate) fn push_duplicate_group_actions(
    resp: &mut OpResponse,
    workspace: &str,
    branch: &str,
    doc: &str,
    mut groups: Vec<DuplicateGroup>,
) {
    // Actions: open helpers for the top duplicate groups (bounded).
    groups.sort_by(|a, b| {
        b.keys
            .len()
            .cmp(&a.keys.len())
            .then_with(|| a.anchor_id.cmp(&b.anchor_id))
            .then_with(|| a.content_hash.cmp(&b.content_hash))
    });
    for group in groups.into_iter().take(5) {
        let ids_limit = group.card_ids.len().clamp(1, 50);
        resp.actions.push(Action {
            action_id: format!(
                "knowledge.lint.duplicate.open::{}::{:016x}",
                group.anchor_id, group.content_hash
            ),
            priority: ActionPriority::High,
            tool: ToolName::GraphOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "graph.query",
                "args": {
                    "workspace": workspace,
                    "branch": branch,
                    "doc": doc,
                    "ids": group.card_ids,
                    "types": ["knowledge"],
                    "limit": ids_limit,
                    "include_edges": false,
                    "edges_limit": 0
                },
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: format!(
                "Открыть дубль-набор для консолидации: {} → k:{} ({} keys).",
                group.anchor_id,
                group.recommended_key,
                group.keys.len()
            ),
            risk: "Низкий".to_string(),
        });
    }
}

pub(crate) fn push_cross_duplicate_group_actions(
    resp: &mut OpResponse,
    workspace: &str,
    branch: &str,
    doc: &str,
    mut groups: Vec<CrossDuplicateGroup>,
) {
    // Actions: open helpers for duplicate content across anchors with multiple keys (bounded).
    groups.sort_by(|a, b| {
        b.anchors
            .len()
            .cmp(&a.anchors.len())
            .then_with(|| b.keys.len().cmp(&a.keys.len()))
            .then_with(|| a.content_hash.cmp(&b.content_hash))
    });
    for group in groups.into_iter().take(3) {
        let ids = group.card_ids.iter().take(30).cloned().collect::<Vec<_>>();
        let ids_limit = ids.len().clamp(1, 50);
        resp.actions.push(Action {
            action_id: format!(
                "knowledge.lint.duplicate.content.open::{:016x}",
                group.content_hash
            ),
            priority: ActionPriority::Medium,
            tool: ToolName::GraphOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "graph.query",
                "args": {
                    "workspace": workspace,
                    "branch": branch,
                    "doc": doc,
                    "ids": ids,
                    "types": ["knowledge"],
                    "limit": ids_limit,
                    "include_edges": false,
                    "edges_limit": 0
                },
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: format!(
                "Открыть duplicate-content across anchors ({} anchors, {} keys) для консолидации: prefer {} → k:{}.",
                group.anchors.len(),
                group.keys.len(),
                group.recommended_anchor_id,
                group.recommended_key
            ),
            risk: "Низкий".to_string(),
        });
    }
}

pub(crate) fn push_overloaded_outliers_actions(
    resp: &mut OpResponse,
    workspace: &str,
    branch: &str,
    doc: &str,
    mut groups: Vec<OverloadedOutliersGroup>,
) {
    // Actions: open helpers for overloaded outliers (dominant-variant cases, bounded).
    groups.sort_by(|a, b| {
        b.total_count
            .cmp(&a.total_count)
            .then_with(|| b.dominant_count.cmp(&a.dominant_count))
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.dominant_hash.cmp(&b.dominant_hash))
    });
    for group in groups.into_iter().take(3) {
        let ids = group
            .outlier_card_ids
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>();
        let ids_limit = ids.len().clamp(1, 50);
        resp.actions.push(Action {
            action_id: format!(
                "knowledge.lint.key.outliers.open::{}::{:016x}",
                group.key, group.dominant_hash
            ),
            priority: ActionPriority::Medium,
            tool: ToolName::GraphOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "graph.query",
                "args": {
                    "workspace": workspace,
                    "branch": branch,
                    "doc": doc,
                    "ids": ids,
                    "types": ["knowledge"],
                    "limit": ids_limit,
                    "include_edges": false,
                    "edges_limit": 0
                },
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: format!(
                "Открыть outliers для k:{} (dominant {} of {}; hash={:016x}).",
                group.key, group.dominant_count, group.total_count, group.dominant_hash
            ),
            risk: "Низкий".to_string(),
        });
    }
}

pub(crate) fn push_overloaded_key_open_actions(
    resp: &mut OpResponse,
    workspace: &str,
    mut keys: Vec<OverloadedKeySummary>,
) {
    keys.sort_by(|a, b| {
        b.anchor_count
            .cmp(&a.anchor_count)
            .then_with(|| b.variant_count.cmp(&a.variant_count))
            .then_with(|| a.key.cmp(&b.key))
    });
    for summary in keys.into_iter().take(3) {
        let key = summary.key;
        resp.actions.push(Action {
            action_id: format!("knowledge.lint.key.open::{key}"),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": workspace,
                "op": "call",
                "cmd": "think.knowledge.query",
                "args": { "key": key, "limit": 20 },
                "budget_profile": "portal",
                "portal_view": "compact"
            }),
            why: format!(
                "Открыть k:{key} across anchors (проверить перегруженность/консолидацию)."
            ),
            risk: "Низкий".to_string(),
        });
    }
}
