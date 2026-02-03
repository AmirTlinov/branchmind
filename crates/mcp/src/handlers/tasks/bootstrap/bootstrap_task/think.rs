#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn maybe_run_think_pipeline(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: &str,
    agent_id: Option<&str>,
    think: Option<Value>,
    warnings: &mut Vec<Value>,
) -> Result<Option<Value>, Value> {
    let Some(think_value) = think else {
        return Ok(None);
    };

    let Some(think_obj) = think_value.as_object() else {
        return Err(ai_error("INVALID_INPUT", "think must be an object"));
    };
    for key in ["target", "branch", "graph_doc", "trace_doc", "notes_doc"] {
        if think_obj.contains_key(key) {
            return Err(ai_error(
                "INVALID_INPUT",
                "think overrides are not supported in tasks_bootstrap",
            ));
        }
    }

    let mut pipeline_args = serde_json::Map::new();
    pipeline_args.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );
    pipeline_args.insert("target".to_string(), Value::String(task_id.to_string()));
    if let Some(agent_id) = think_obj.get("agent_id") {
        pipeline_args.insert("agent_id".to_string(), agent_id.clone());
    } else if let Some(agent_id) = agent_id {
        pipeline_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
    } else if let Some(agent_id) = server.default_agent_id.as_deref() {
        pipeline_args.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
    }
    for key in [
        "frame",
        "hypothesis",
        "test",
        "evidence",
        "decision",
        "status",
        "note_decision",
        "note_title",
        "note_format",
    ] {
        if let Some(value) = think_obj.get(key) {
            pipeline_args.insert(key.to_string(), value.clone());
        }
    }

    let pipeline_response = server.tool_branchmind_think_pipeline(Value::Object(pipeline_args));
    if pipeline_response
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        if let Some(pipeline_warnings) =
            pipeline_response.get("warnings").and_then(|v| v.as_array())
        {
            warnings.extend(pipeline_warnings.clone());
        }
        Ok(pipeline_response.get("result").cloned())
    } else {
        let message = pipeline_response
            .get("error")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Think pipeline failed");
        warnings.push(warning(
            "THINK_PIPELINE_FAILED",
            message,
            &format!(
                "Call think_pipeline with target={} to seed reasoning.",
                task_id
            ),
        ));
        Ok(Some(json!({
            "ok": false,
            "error": pipeline_response.get("error").cloned().unwrap_or(Value::Null)
        })))
    }
}
