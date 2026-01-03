#![forbid(unsafe_code)]

use super::super::super::StoreError;
use bm_core::graph::{ConflictId, GraphNodeId, GraphRel, GraphType};

pub(in crate::store) fn validate_graph_node_id(value: &str) -> Result<(), StoreError> {
    GraphNodeId::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

pub(in crate::store) fn validate_graph_type(value: &str) -> Result<(), StoreError> {
    GraphType::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

pub(in crate::store) fn validate_graph_rel(value: &str) -> Result<(), StoreError> {
    GraphRel::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}

pub(in crate::store) fn validate_conflict_id(value: &str) -> Result<(), StoreError> {
    ConflictId::try_new(value)
        .map(|_| ())
        .map_err(|err| StoreError::InvalidInput(err.message()))
}
