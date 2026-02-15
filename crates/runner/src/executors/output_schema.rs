#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn role_slug(role: Option<&str>) -> &'static str {
    let Some(role) = role.map(str::trim) else {
        return "default";
    };
    if role.eq_ignore_ascii_case("scout") {
        "scout"
    } else if role.eq_ignore_ascii_case("builder") {
        "builder"
    } else if role.eq_ignore_ascii_case("validator") {
        "validator"
    } else if role.eq_ignore_ascii_case("writer") {
        "writer"
    } else {
        "default"
    }
}

fn scout_summary_schema() -> Value {
    // NOTE: This schema is intentionally *structural*, not semantic.
    // We keep it small so cheap scout models (e.g. Haiku) reliably return valid JSON.
    // Semantic/SSOT validation happens in bm_mcp (store-backed), not here.
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "objective": { "type": "string", "minLength": 8 },
            "scope": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "in": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "out": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
                },
                "required": ["in", "out"]
            },
            "anchors": {
                "type": "array",
                "minItems": 3,
                "maxItems": 64,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "minLength": 3, "maxLength": 96 },
                        "rationale": { "type": "string", "minLength": 12, "maxLength": 240 },
                        "anchor_type": {
                            "type": "string",
                            "enum": ["primary", "dependency", "reference", "structural"]
                        },
                        "code_ref": {
                            "type": "string",
                            "pattern": "^code:.+#L[1-9][0-9]*-L[1-9][0-9]*(?:@sha256:[0-9a-fA-F]{64})?$"
                        },
                        "content": { "type": "string", "minLength": 1, "maxLength": 500 },
                        "line_count": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "required": ["id", "rationale", "anchor_type", "code_ref", "content", "line_count"]
                }
            },
            "code_refs": {
                "type": "array",
                "minItems": 3,
                "maxItems": 64,
                "items": {
                    "type": "string",
                    "pattern": "^code:.+#L[1-9][0-9]*-L[1-9][0-9]*(?:@sha256:[0-9a-fA-F]{64})?$"
                }
            },
            "change_hints": {
                "type": "array",
                "minItems": 2,
                "maxItems": 16,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string", "minLength": 2, "maxLength": 220 },
                        "intent": { "type": "string", "minLength": 4, "maxLength": 240 },
                        "risk": { "type": "string", "minLength": 4, "maxLength": 160 }
                    },
                    "required": ["path", "intent", "risk"]
                }
            },
            "test_hints": {
                "type": "array",
                "minItems": 3,
                "maxItems": 24,
                "items": {
                    "anyOf": [
                        { "type": "string", "minLength": 4, "maxLength": 240 },
                        {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "name": { "type": "string", "minLength": 4, "maxLength": 140 },
                                "intent": { "type": "string", "minLength": 4, "maxLength": 220 }
                            },
                            "required": ["name", "intent"]
                        }
                    ]
                }
            },
            "risk_map": {
                "type": "array",
                "minItems": 3,
                "maxItems": 24,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "risk": { "type": "string", "minLength": 4, "maxLength": 220 },
                        "falsifier": { "type": "string", "minLength": 4, "maxLength": 260 }
                    },
                    "required": ["risk", "falsifier"]
                }
            },
            "open_questions": {
                "type": "array",
                "maxItems": 16,
                "items": { "type": "string", "maxLength": 220 }
            },
            "summary_for_builder": { "type": "string", "minLength": 320, "maxLength": 1200 },
            "coverage_matrix": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "objective_items": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 16,
                        "items": { "type": "string", "minLength": 2, "maxLength": 180 }
                    },
                    "change_hint_coverage": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 32,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "path": { "type": "string", "minLength": 2, "maxLength": 220 },
                                "primary_or_structural_anchor_ids": {
                                    "type": "array",
                                    "minItems": 1,
                                    "maxItems": 16,
                                    "items": { "type": "string", "minLength": 2, "maxLength": 96 }
                                },
                                "status": { "type": "string", "enum": ["covered", "needs_more_context"] }
                            },
                            "required": ["path", "primary_or_structural_anchor_ids", "status"]
                        }
                    }
                },
                "required": ["objective_items", "change_hint_coverage"]
            },
            // Most optional fields are deliberately excluded from runner schema to avoid
            // structured-output retry loops on small models.
            // coverage_matrix stays optional but is included to codify change_hint↔anchor coverage.
        },
        "required": [
            "objective",
            "scope",
            "anchors",
            "code_refs",
            "change_hints",
            "test_hints",
            "risk_map",
            "open_questions",
            "summary_for_builder",
            "coverage_matrix"
        ]
    })
}

fn builder_summary_schema() -> Value {
    let with_context = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "slice_id": { "type": "string", "minLength": 3 },
            "changes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string", "minLength": 2 },
                        "intent": { "type": "string", "minLength": 4 },
                        "diff_ref": { "type": "string", "minLength": 4 },
                        "estimated_risk": { "type": "string", "minLength": 3 }
                    },
                    "required": ["path", "intent", "diff_ref", "estimated_risk"]
                }
            },
            "context_request": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "reason": { "type": "string", "minLength": 4 },
                    "missing_context": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 2 } },
                    "suggested_scout_focus": { "type": "array", "items": { "type": "string", "minLength": 2 } },
                    "suggested_tests": { "type": "array", "items": { "type": "string", "minLength": 2 } }
                },
                "required": ["reason", "missing_context", "suggested_scout_focus", "suggested_tests"]
            },
            "checks_to_run": { "type": "array", "items": { "type": "string", "minLength": 2 } },
            "rollback_plan": { "type": "string", "minLength": 8 },
            "proof_refs": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string", "pattern": "^(CMD:|LINK:|FILE:).+" }
            },
            "execution_evidence": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "revision": { "type": "integer", "minimum": 1 },
                    "diff_scope": { "type": "array", "items": { "type": "string", "minLength": 2 } },
                    "command_runs": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "cmd": { "type": "string", "minLength": 2 },
                                "exit_code": { "type": "integer" },
                                "stdout_ref": { "type": "string", "minLength": 4 },
                                "stderr_ref": { "type": "string", "minLength": 4 }
                            },
                            "required": ["cmd", "exit_code", "stdout_ref", "stderr_ref"]
                        }
                    },
                    "rollback_proof": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "strategy": { "type": "string", "minLength": 2 },
                            "target_revision": { "type": "integer" },
                            "verification_cmd_ref": {
                                "type": "string",
                                "pattern": "^(CMD:|LINK:|FILE:).+"
                            }
                        },
                        "required": ["strategy", "target_revision", "verification_cmd_ref"]
                    },
                    "semantic_guards": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "must_should_may_delta": { "type": "string", "minLength": 2 },
                            "contract_term_consistency": { "type": "string", "minLength": 2 }
                        },
                        "required": ["must_should_may_delta", "contract_term_consistency"]
                    }
                },
                "required": ["revision", "diff_scope", "command_runs", "rollback_proof", "semantic_guards"]
            }
        },
        "required": ["slice_id", "changes", "checks_to_run", "context_request", "rollback_plan", "proof_refs", "execution_evidence"]
    });

    let mut without_context = with_context.clone();
    if let Some(props) = without_context
        .get_mut("properties")
        .and_then(|v| v.as_object_mut())
    {
        props.remove("context_request");
    }
    if let Some(req) = without_context
        .get_mut("required")
        .and_then(|v| v.as_array_mut())
    {
        req.retain(|item| item.as_str() != Some("context_request"));
    }

    json!({ "anyOf": [without_context, with_context] })
}

fn validator_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "slice_id": { "type": "string", "minLength": 3 },
            "plan_fit_score": { "type": "integer", "minimum": 0, "maximum": 100 },
            "policy_checks": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": { "type": "string", "minLength": 2 },
                        "pass": { "type": "boolean" },
                        "reason": { "type": "string", "minLength": 2 }
                    },
                    "required": ["name", "pass", "reason"]
                }
            },
            "tests": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": { "type": "string", "minLength": 2 },
                        "pass": { "type": "boolean" },
                        "evidence_ref": { "type": "string", "minLength": 2 }
                    },
                    "required": ["name", "pass", "evidence_ref"]
                }
            },
            "security_findings": { "type": "array", "items": { "type": "object" } },
            "regression_risk": { "type": "string", "enum": ["low", "medium", "high"] },
            "recommendation": { "type": "string", "enum": ["approve", "rework", "reject"] },
            "rework_actions": { "type": "array", "items": { "type": "string", "minLength": 2 } }
        },
        "required": [
            "slice_id",
            "plan_fit_score",
            "policy_checks",
            "tests",
            "security_findings",
            "regression_risk",
            "recommendation",
            "rework_actions"
        ]
    })
}

fn writer_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "slice_id": { "type": "string", "minLength": 3 },
            "patches": { "type": "array", "items": { "type": "object" } },
            "summary": { "type": "string", "minLength": 4 },
            "affected_files": { "type": "array", "items": { "type": "string", "minLength": 2 } },
            "checks_to_run": { "type": "array", "items": { "type": "string", "minLength": 2 } },
            "insufficient_context": { "type": "string" }
        },
        "required": [
            "slice_id",
            "patches",
            "summary",
            "affected_files",
            "checks_to_run",
            "insufficient_context"
        ]
    })
}

fn summary_schema_for_role(role: Option<&str>) -> Value {
    match role_slug(role) {
        "scout" => scout_summary_schema(),
        "builder" => builder_summary_schema(),
        "validator" => validator_summary_schema(),
        "writer" => writer_summary_schema(),
        _ => json!({ "type": "string" }),
    }
}

pub(crate) fn job_output_schema_value_for_role(role: Option<&str>) -> Value {
    // Minimal structured contract: the runner only needs status + summary + stable refs.
    // We allow CONTINUE so multi-hour jobs can be time-sliced safely.
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "status": { "type": "string", "enum": ["DONE", "FAILED", "CONTINUE"] },
            "summary": summary_schema_for_role(role),
            "refs": { "type": "array", "items": { "type": "string" } },
            "events": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": { "type": "string" },
                        "message": { "type": "string" },
                        "percent": { "type": "integer" },
                        "refs": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["kind", "message", "percent", "refs"]
                }
            }
        },
        "required": ["events", "refs", "status", "summary"]
    })
}

pub(crate) fn job_output_schema_json_arg_for_role(role: Option<&str>) -> Result<String, String> {
    serde_json::to_string(&job_output_schema_value_for_role(role))
        .map_err(|e| format!("serialize output schema failed: {e}"))
}

pub(crate) fn write_job_output_schema_file_for_role(
    tmp_dir: &Path,
    role: Option<&str>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(tmp_dir).map_err(|e| format!("tmp dir create failed: {e}"))?;
    let slug = role_slug(role);
    let file_name = if slug == "default" {
        "output_schema.json".to_string()
    } else {
        format!("output_schema_{slug}.json")
    };
    let schema_path = tmp_dir.join(file_name);
    let schema = job_output_schema_value_for_role(role);

    std::fs::write(
        &schema_path,
        serde_json::to_vec_pretty(&schema).unwrap_or_default(),
    )
    .map_err(|e| format!("write schema failed: {e}"))?;

    Ok(schema_path)
}
