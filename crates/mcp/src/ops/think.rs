#![forbid(unsafe_code)]

mod handlers;
mod knowledge_lint;
mod knowledge_search;
mod note_promote;
mod register;

pub(crate) use handlers::{KB_BRANCH, KB_GRAPH_DOC, fnv1a64, normalized_claim};
pub(crate) use register::register;
