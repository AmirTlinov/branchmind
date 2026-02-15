#![forbid(unsafe_code)]

use super::super::ai::ai_error;
use bm_core::ids::WorkspaceId;
use serde_json::Value;

pub(crate) fn require_workspace(
    args: &serde_json::Map<String, Value>,
) -> Result<WorkspaceId, Value> {
    let Some(v) = args.get("workspace").and_then(|v| v.as_str()) else {
        return Err(ai_error("INVALID_INPUT", "workspace is required"));
    };
    match WorkspaceId::try_new(v.to_string()) {
        Ok(w) => Ok(w),
        Err(_) => Err(ai_error(
            "INVALID_INPUT",
            "workspace: expected WorkspaceId (e.g. \"money1\"). Tip: you may also pass an absolute path (e.g. \"/home/me/repo\") and it will be mapped to an id.",
        )),
    }
}
