#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(crate) fn job_output_schema_value() -> Value {
    // Minimal structured contract: the runner only needs status + summary + stable refs.
    // We allow CONTINUE so multi-hour jobs can be time-sliced safely.
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "status": { "type": "string", "enum": ["DONE", "FAILED", "CONTINUE"] },
            "summary": { "type": "string" },
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

pub(crate) fn job_output_schema_json_arg() -> Result<String, String> {
    serde_json::to_string(&job_output_schema_value())
        .map_err(|e| format!("serialize output schema failed: {e}"))
}

pub(crate) fn write_job_output_schema_file(tmp_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(tmp_dir).map_err(|e| format!("tmp dir create failed: {e}"))?;
    let schema_path = tmp_dir.join("output_schema.json");

    let schema = job_output_schema_value();

    std::fs::write(
        &schema_path,
        serde_json::to_vec_pretty(&schema).unwrap_or_default(),
    )
    .map_err(|e| format!("write schema failed: {e}"))?;

    Ok(schema_path)
}
