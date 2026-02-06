#![forbid(unsafe_code)]

mod derive;
mod evidence;
mod filter;
mod step_aware;
mod text;
mod types;
mod util;

pub(crate) const REASONING_ENGINE_VERSION: &str = "v0.5";

pub(crate) use derive::derive_reasoning_engine;
pub(crate) use filter::filter_engine_to_cards;
pub(crate) use step_aware::derive_reasoning_engine_step_aware;
pub(crate) use types::{EngineLimits, EngineScope};
