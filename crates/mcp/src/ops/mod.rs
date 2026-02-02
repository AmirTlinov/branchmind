#![forbid(unsafe_code)]

mod actions;
mod dispatch;
mod docs;
mod envelope;
mod graph;
mod jobs;
mod legacy_bridge;
mod next_engine;
mod normalize;
mod recovery;
mod registry;
mod schema;
mod system;
mod tasks;
mod think;
mod vcs;
mod workspace;

pub(crate) use actions::*;
pub(crate) use dispatch::*;
pub(crate) use envelope::*;
pub(crate) use legacy_bridge::legacy_to_op_response;
pub(crate) use next_engine::*;
pub(crate) use normalize::*;
pub(crate) use registry::*;
pub(crate) use schema::*;

#[cfg(test)]
mod docs_guard;

#[cfg(test)]
mod tests;
