#![forbid(unsafe_code)]
//! Graph tools (split-friendly module root).

mod merge;
mod ops;
mod resolve;
mod scope;
mod think_commit;

pub(super) use think_commit::ThinkCardCommitInternalArgs;
