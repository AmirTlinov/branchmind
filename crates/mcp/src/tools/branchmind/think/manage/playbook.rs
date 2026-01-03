#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_playbook(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let name = match require_string(args_obj, "name") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let template = match name.as_str() {
            "default" => json!({
                "steps": [
                    "frame: clarify intent, constraints, and success criteria",
                    "hypothesis: list likely explanations",
                    "test: design the smallest safe probe",
                    "evidence: capture results",
                    "decision: commit the next action"
                ]
            }),
            "debug" => json!({
                "steps": [
                    "frame: reproduce and isolate the failure",
                    "hypothesis: enumerate causes by layer",
                    "test: shrink to a minimal repro",
                    "evidence: capture logs/traces",
                    "decision: fix + verify"
                ]
            }),
            _ => json!({
                "steps": [
                    "frame: clarify the goal",
                    "hypothesis: list options",
                    "test: choose the smallest check",
                    "evidence: record outcomes",
                    "decision: commit the path forward"
                ]
            }),
        };

        let mut result = json!({
            "workspace": workspace.as_str(),
            "name": name,
            "template": template,
            "truncated": false
        });

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if let Some(obj) = value.as_object_mut() {
                        changed |= obj.remove("template").is_some();
                    }
                    changed
                });

            set_truncated_flag(&mut result, truncated);
            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok("think_playbook", result)
        } else {
            ai_ok_with_warnings("think_playbook", result, warnings, Vec::new())
        }
    }
}
