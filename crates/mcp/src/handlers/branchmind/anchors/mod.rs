#![forbid(unsafe_code)]

mod bootstrap;
mod export;
mod lint;
mod list;
mod macro_note;
mod merge;
mod rename;
mod resolve;
mod snapshot;

pub(super) const ANCHORS_GRAPH_DOC: &str = "anchors-graph";
pub(super) const ANCHORS_TRACE_DOC: &str = "anchors-trace";
