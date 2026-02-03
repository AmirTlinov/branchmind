#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) fn apply_lane_context_to_card(
    args_obj: &serde_json::Map<String, Value>,
    parsed: &mut ParsedThinkCard,
) -> Result<(), Value> {
    let _agent_id = optional_agent_id(args_obj, "agent_id")?;
    // Meaning-mode: durable artifacts are stored shared-by-default.
    // Legacy lane tags remain readable as draft markers, but new writes do not depend on `agent_id`.
    apply_lane_stamp_to_tags(&mut parsed.tags, None);
    apply_lane_stamp_to_meta(&mut parsed.meta_value, None);
    Ok(())
}
