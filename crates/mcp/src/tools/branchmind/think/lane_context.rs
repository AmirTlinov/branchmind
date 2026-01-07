#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) fn apply_lane_context_to_card(
    args_obj: &serde_json::Map<String, Value>,
    parsed: &mut ParsedThinkCard,
) -> Result<(), Value> {
    let agent_id = optional_agent_id(args_obj, "agent_id")?;
    apply_lane_stamp_to_tags(&mut parsed.tags, agent_id.as_deref());
    apply_lane_stamp_to_meta(&mut parsed.meta_value, agent_id.as_deref());
    Ok(())
}
