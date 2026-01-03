#![forbid(unsafe_code)]
//! High-signal task context snapshots (super resume, snapshot, context pack).

mod budget;
mod graph_diff;
mod resume_super;
mod wrappers;
