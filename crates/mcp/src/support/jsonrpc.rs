#![forbid(unsafe_code)]

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcRequest {
    #[serde(default)]
    #[serde(rename = "jsonrpc")]
    pub(crate) _jsonrpc: Option<String>,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) id: Option<Value>,
    #[serde(default)]
    pub(crate) params: Option<Value>,
}

pub(crate) fn json_rpc_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub(crate) fn json_rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub(crate) fn tool_text_content(payload: &Value) -> Value {
    // AI-agent UX: when a tool intentionally renders compact tagged lines (BM-L1),
    // return that raw string directly to avoid wasting tokens on a JSON envelope.
    //
    // Structured payloads remain available via explicit full-view tools; portals stay context-first.
    if payload
        .get("line_protocol")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && let Some(rendered) = payload.get("result").and_then(|v| v.as_str())
    {
        return Value::Object(
            [
                ("type".to_string(), Value::String("text".to_string())),
                ("text".to_string(), Value::String(rendered.to_string())),
            ]
            .into_iter()
            .collect(),
        );
    }

    if let Some(rendered) = payload.get("result").and_then(|v| v.as_str())
        && looks_like_bm_line_protocol(rendered)
    {
        return Value::Object(
            [
                ("type".to_string(), Value::String("text".to_string())),
                ("text".to_string(), Value::String(rendered.to_string())),
            ]
            .into_iter()
            .collect(),
        );
    }

    Value::Object(
        [
            ("type".to_string(), Value::String("text".to_string())),
            (
                "text".to_string(),
                Value::String(
                    serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string()),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn looks_like_bm_line_protocol(text: &str) -> bool {
    // We intentionally keep this heuristic strict to avoid breaking tools that return arbitrary
    // strings. A BM-L1 payload is tag-light: at least one line begins with a known BM tag.
    text.lines().any(|line| {
        line.starts_with("ERROR: ") || line.starts_with("WARNING: ") || line.starts_with("MORE: ")
    })
}
