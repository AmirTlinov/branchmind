#![forbid(unsafe_code)]

use super::super::super::*;

pub(super) struct ValidatedThinkCardCommit {
    pub(super) branch: String,
    pub(super) trace_doc: String,
    pub(super) graph_doc: String,
    pub(super) card_id: String,
    pub(super) card_type: String,
    pub(super) card: ThinkCardInput,
    pub(super) tags: Vec<String>,
    pub(super) supports: Vec<String>,
    pub(super) blocks: Vec<String>,
}

pub(super) fn validate(
    request: ThinkCardCommitRequest,
) -> Result<ValidatedThinkCardCommit, StoreError> {
    let ThinkCardCommitRequest {
        branch,
        trace_doc,
        graph_doc,
        card,
        supports,
        blocks,
    } = request;

    let branch = branch.trim();
    if branch.is_empty() {
        return Err(StoreError::InvalidInput("branch must not be empty"));
    }
    let trace_doc = trace_doc.trim();
    if trace_doc.is_empty() {
        return Err(StoreError::InvalidInput("trace_doc must not be empty"));
    }
    let graph_doc = graph_doc.trim();
    if graph_doc.is_empty() {
        return Err(StoreError::InvalidInput("graph_doc must not be empty"));
    }

    let card_id = card.card_id.trim();
    if card_id.is_empty() {
        return Err(StoreError::InvalidInput("card.id must not be empty"));
    }
    let card_type = card.card_type.trim();
    if card_type.is_empty() {
        return Err(StoreError::InvalidInput("card.type must not be empty"));
    }
    if !bm_core::think::is_supported_think_card_type(card_type) {
        return Err(StoreError::InvalidInput("unsupported card.type"));
    }
    validate_graph_node_id(card_id)?;
    validate_graph_type(card_type)?;

    let title_ok = card
        .title
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let text_ok = card
        .text
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if !title_ok && !text_ok {
        return Err(StoreError::InvalidInput(
            "card must have at least one of title or text",
        ));
    }

    let tags = normalize_tags(&card.tags)?;

    Ok(ValidatedThinkCardCommit {
        branch: branch.to_string(),
        trace_doc: trace_doc.to_string(),
        graph_doc: graph_doc.to_string(),
        card_id: card_id.to_string(),
        card_type: card_type.to_string(),
        card,
        tags,
        supports,
        blocks,
    })
}
