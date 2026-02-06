#![forbid(unsafe_code)]
//! Reasoning gate for step closure.
//!
//! This gate is **opt-in** via `task.reasoning_mode` and is designed to keep the system
//! "agent-first": it blocks unsafe closure patterns but provides portal-first recovery
//! suggestions (and an explicit override escape hatch in the macro flow).

use crate::*;
use serde_json::Value;

mod deep;
mod discipline;
mod gate;
mod override_apply;
mod spec;

pub(crate) use gate::enforce_reasoning_gate;
pub(crate) use override_apply::{ReasoningOverride, parse_reasoning_override};

pub(crate) struct ReasoningGateContext<'a> {
    pub(crate) server: &'a mut McpServer,
    pub(crate) workspace: &'a WorkspaceId,
    pub(crate) task_id: &'a str,
    pub(crate) step_id: Option<&'a str>,
    pub(crate) path: Option<&'a StepPath>,
    pub(crate) args_obj: &'a serde_json::Map<String, Value>,
    pub(crate) reasoning_override: Option<&'a ReasoningOverride>,
    pub(crate) allow_override: bool,
    pub(crate) close_args_obj: Option<&'a mut serde_json::Map<String, Value>>,
    pub(crate) warnings: Option<&'a mut Vec<Value>>,
    pub(crate) note_event: Option<&'a mut Option<Value>>,
}
