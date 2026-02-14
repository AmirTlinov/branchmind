#![forbid(unsafe_code)]

mod cascade;
mod context_review;
mod dispatch_writer;
mod pre_validate;

pub(super) use super::pipeline::{
    MeshMessageRequest, optional_non_empty_string, parse_meta_map, publish_optional_mesh_message,
    require_non_empty_string, scout_policy_from_meta,
};

pub(super) const DEFAULT_JOBS_MODEL: &str = "gpt-5.3-codex";
pub(super) const DEFAULT_EXECUTOR_PROFILE: &str = "xhigh";
pub(super) const DEFAULT_CONTEXT_REVIEW_MODE: &str = "haiku_fast";
