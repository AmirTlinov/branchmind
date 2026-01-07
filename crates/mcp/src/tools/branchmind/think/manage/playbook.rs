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
                    "pattern: if search space is ordered, prefer bisect",
                    "evidence: capture logs/traces",
                    "decision: fix + verify"
                ]
            }),
            "bisect" => json!({
                "steps": [
                    "frame: define the failing signal (red) and success signal (green)",
                    "scope: choose bisect axis (commit range / flags / config)",
                    "setup: ensure each run is deterministic and cheap",
                    "loop: pick midpoint → run → label good/bad",
                    "evidence: capture the smallest proof for the pivot point",
                    "decision: commit the fix and lock a regression test"
                ]
            }),
            "criteria_matrix" => json!({
                "steps": [
                    "frame: state the decision as a one-line question",
                    "options: list 2–5 options (A/B/...) with one-line descriptions",
                    "criteria: define 5–9 criteria + weights (1–5); keep them measurable",
                    "matrix: score each option per criterion (0–5) and write the reason",
                    "sensitivity: change top weights and see if winner flips",
                    "decision: pick winner + explicitly record tradeoffs",
                    "next: define 1 primary executable step + 1 backup step"
                ],
                "matrix_template": {
                    "options": ["A", "B"],
                    "criteria": [
                        { "name": "correctness", "weight": 5 },
                        { "name": "complexity", "weight": 3 }
                    ]
                }
            }),
            "experiment" => json!({
                "steps": [
                    "frame: state what you want to learn (one sentence)",
                    "hypothesis: what you believe and why",
                    "experiment: design the smallest decisive test (runnable if possible)",
                    "prediction: what results would support vs refute",
                    "evidence: capture CMD + LINK receipts",
                    "decision: update plan and pin the conclusion if durable"
                ]
            }),
            "contradiction" => json!({
                "steps": [
                    "frame: name the contradiction as supports vs blocks",
                    "evidence: list the strongest items on each side (with receipts if possible)",
                    "disambiguate: define ONE decisive test that can break the tie",
                    "run: execute outside BranchMind (CI/local) and capture receipts",
                    "decision: resolve by pinning the winning claim and linking evidence"
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
