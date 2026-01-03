#![forbid(unsafe_code)]

use super::super::super::{GraphEdgeRow, GraphNodeRow};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::store) struct GraphEdgeKey {
    pub(in crate::store) from: String,
    pub(in crate::store) rel: String,
    pub(in crate::store) to: String,
}

#[derive(Clone, Debug)]
pub(in crate::store) enum GraphDiffCandidate {
    Node { to: GraphNodeRow },
    Edge { key: GraphEdgeKey, to: GraphEdgeRow },
}

impl GraphDiffCandidate {
    pub(in crate::store) fn last_seq(&self) -> i64 {
        match self {
            Self::Node { to } => to.last_seq,
            Self::Edge { to, .. } => to.last_seq,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::store) enum GraphMergeCandidate {
    Node { theirs: GraphNodeRow },
    Edge { theirs: GraphEdgeRow },
}

impl GraphMergeCandidate {
    pub(in crate::store) fn last_seq(&self) -> i64 {
        match self {
            Self::Node { theirs } => theirs.last_seq,
            Self::Edge { theirs } => theirs.last_seq,
        }
    }
}

pub(in crate::store) struct GraphNodeVersionInsertTxArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) seq: i64,
    pub(in crate::store) ts_ms: i64,
    pub(in crate::store) node_id: &'a str,
    pub(in crate::store) node_type: Option<&'a str>,
    pub(in crate::store) title: Option<&'a str>,
    pub(in crate::store) text: Option<&'a str>,
    pub(in crate::store) tags: &'a [String],
    pub(in crate::store) status: Option<&'a str>,
    pub(in crate::store) meta_json: Option<&'a str>,
    pub(in crate::store) deleted: bool,
}

pub(in crate::store) struct GraphEdgeVersionInsertTxArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) seq: i64,
    pub(in crate::store) ts_ms: i64,
    pub(in crate::store) from_id: &'a str,
    pub(in crate::store) rel: &'a str,
    pub(in crate::store) to_id: &'a str,
    pub(in crate::store) meta_json: Option<&'a str>,
    pub(in crate::store) deleted: bool,
}

pub(in crate::store) struct GraphNodeUpsertTxArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) now_ms: i64,
    pub(in crate::store) node_id: &'a str,
    pub(in crate::store) node_type: &'a str,
    pub(in crate::store) title: Option<&'a str>,
    pub(in crate::store) status: Option<&'a str>,
    pub(in crate::store) meta_json: Option<&'a str>,
    pub(in crate::store) source_event_id: &'a str,
}

pub(in crate::store) struct GraphEdgeUpsertTxArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) now_ms: i64,
    pub(in crate::store) from: &'a str,
    pub(in crate::store) rel: &'a str,
    pub(in crate::store) to: &'a str,
    pub(in crate::store) meta_json: Option<&'a str>,
    pub(in crate::store) source_event_id: &'a str,
}

pub(in crate::store) struct GraphConflictIdArgs<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) from_branch: &'a str,
    pub(in crate::store) into_branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) kind: &'a str,
    pub(in crate::store) key: &'a str,
    pub(in crate::store) base_cutoff_seq: i64,
    pub(in crate::store) theirs_seq: i64,
    pub(in crate::store) ours_seq: i64,
}

pub(in crate::store) struct GraphConflictPreviewCtx<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) from_branch: &'a str,
    pub(in crate::store) into_branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) base_cutoff_seq: i64,
    pub(in crate::store) now_ms: i64,
    pub(in crate::store) status: &'a str,
}

pub(in crate::store) struct GraphConflictCreateCtx<'a> {
    pub(in crate::store) workspace: &'a str,
    pub(in crate::store) from_branch: &'a str,
    pub(in crate::store) into_branch: &'a str,
    pub(in crate::store) doc: &'a str,
    pub(in crate::store) base_cutoff_seq: i64,
    pub(in crate::store) now_ms: i64,
}
