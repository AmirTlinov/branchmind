#![forbid(unsafe_code)]

use crate::ops::Action;
use crate::support::now_rfc3339;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct Envelope {
    pub(crate) workspace: Option<String>,
    pub(crate) cmd: String,
    pub(crate) args: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct OpError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) recovery: Option<String>,
}

impl OpError {
    pub(crate) fn to_value(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("code".to_string(), Value::String(self.code.clone()));
        obj.insert("message".to_string(), Value::String(self.message.clone()));
        if let Some(recovery) = &self.recovery {
            obj.insert("recovery".to_string(), Value::String(recovery.clone()));
        }
        Value::Object(obj)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OpResponse {
    pub(crate) intent: String,
    pub(crate) result: Value,
    pub(crate) refs: Vec<String>,
    pub(crate) warnings: Vec<Value>,
    pub(crate) actions: Vec<Action>,
    pub(crate) error: Option<OpError>,
}

impl OpResponse {
    pub(crate) fn success(intent: String, result: Value) -> Self {
        Self {
            intent,
            result,
            refs: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn error(intent: String, error: OpError) -> Self {
        Self {
            intent,
            result: json!({}),
            refs: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
            error: Some(error),
        }
    }

    pub(crate) fn into_value(mut self) -> Value {
        self.actions.sort_by(|a, b| {
            a.priority
                .rank()
                .cmp(&b.priority.rank())
                .then_with(|| a.action_id.cmp(&b.action_id))
        });
        json!({
            "success": self.error.is_none(),
            "intent": self.intent,
            "result": self.result,
            "refs": if self.refs.is_empty() { Value::Null } else { Value::Array(self.refs.into_iter().map(Value::String).collect()) },
            "actions": self.actions.iter().map(|a| a.to_json()).collect::<Vec<_>>(),
            "warnings": self.warnings,
            "suggestions": [],
            "context": {},
            "error": self.error.as_ref().map(|e| e.to_value()).unwrap_or(Value::Null),
            "timestamp": now_rfc3339(),
        })
    }
}

pub(crate) fn error_unknown_tool(name: &str) -> Value {
    OpResponse::error(
        "error".to_string(),
        OpError {
            code: "UNKNOWN_TOOL".to_string(),
            message: format!("Unknown tool: {name}"),
            recovery: Some("Use tools/list (allowed tools: think, branch, merge).".to_string()),
        },
    )
    .into_value()
}

pub(crate) fn error_internal(message: String) -> Value {
    OpResponse::error(
        "error".to_string(),
        OpError {
            code: "INTERNAL_ERROR".to_string(),
            message,
            recovery: Some(
                "Retry the call. If it repeats, restart the server and capture logs.".to_string(),
            ),
        },
    )
    .into_value()
}
