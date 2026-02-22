#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn new(store: bm_storage::SqliteStore) -> Self {
        Self {
            initialized: false,
            store,
        }
    }

    pub(crate) fn handle(&mut self, request: crate::JsonRpcRequest) -> Option<Value> {
        let method = request.method.as_str();
        let expects_response = !matches!(request.id.as_ref(), None | Some(Value::Null));

        if method == "initialize" {
            // MCP protocol negotiation:
            // Some clients are strict about the server echoing the chosen protocol version.
            // We remain forward-compatible by accepting the client’s declared version and
            // reflecting it back (fallback to our baseline when absent).
            let protocol_version = request
                .params
                .as_ref()
                .and_then(|v| v.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or(crate::MCP_VERSION);

            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "protocolVersion": protocol_version,
                    "serverInfo": {
                        "name": crate::SERVER_NAME,
                        "version": crate::build_fingerprint()
                    },
                    // MCP polish: advertise the optional surfaces we implement as deterministic,
                    // empty stubs. Some clients probe these by default and may treat "method not
                    // found" as a hard failure.
                    "capabilities": {
                        "tools": {},
                        "resources": {},
                        "prompts": {},
                        "logging": {}
                    }
                }),
            ));
        }

        // MCP client compatibility:
        // - The spec uses `notifications/initialized`.
        // - Some clients send `initialized` as a plain notification.
        // We accept both and never respond (notification).
        if method == "notifications/initialized" || method == "initialized" {
            self.initialized = true;
            return None;
        }

        if !self.initialized {
            // Out-of-box DX: allow auto-initialization on first real request. This avoids
            // client startup races that would otherwise yield "Server not initialized".
            if matches!(
                method,
                "tools/call"
                    | "tools/list"
                    | "resources/list"
                    | "resources/read"
                    | "resources/templates/list"
                    | "ping"
            ) {
                self.initialized = true;
            } else if expects_response {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32002,
                    "Server not initialized",
                ));
            } else {
                // Unknown notification before initialization: ignore.
                return None;
            }
        }

        if method == "ping" {
            return Some(crate::json_rpc_response(request.id, json!({})));
        }

        // MCP polish: some clients probe optional resources methods by default. We keep the
        // surface deterministic and minimal by advertising an empty resource set.
        if method == "resources/list" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "resources": [] }),
            ));
        }
        if method == "resources/templates/list" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "resourceTemplates": [] }),
            ));
        }
        if method == "resources/read" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "contents": [] }),
            ));
        }

        // MCP client compatibility: optional surfaces that some clients call unconditionally.
        if method == "prompts/list" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "prompts": [] }),
            ));
        }
        if method == "prompts/get" {
            return Some(crate::json_rpc_error(request.id, -32602, "Unknown prompt"));
        }
        if method == "logging/setLevel" {
            return Some(crate::json_rpc_response(request.id, json!({})));
        }
        if method == "roots/list" {
            return Some(crate::json_rpc_response(request.id, json!({ "roots": [] })));
        }

        if method == "tools/list" {
            // v3: strict surface = 3 markdown tools.
            let tools = crate::tools::tool_definitions();
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "tools": tools }),
            ));
        }

        if method == "tools/call" {
            let Some(params) = request.params else {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };
            let Some(params_obj) = params.as_object() else {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32602,
                    "params must be an object",
                ));
            };

            let tool_name = params_obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // MCP client interop: some clients send `"arguments": null` for empty-args tools.
            // Treat missing/null as `{}` but keep non-object values as-is so tool validators
            // can return a precise `INVALID_INPUT` error.
            let args = match params_obj.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(v) => v.clone(),
            };
            let response_body = self.call_tool(tool_name, args);

            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "content": [crate::tool_text_content(&response_body)],
                    "isError": !response_body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                }),
            ));
        }

        // Notifications (no id / id=null) must not receive a response, even if unknown.
        if !expects_response {
            return None;
        }

        Some(crate::json_rpc_error(
            request.id,
            -32601,
            &format!("Method not found: {method}"),
        ))
    }

    pub(crate) fn call_tool(&mut self, name: &str, args: Value) -> Value {
        // v3: only the 3 advertised markdown tools are callable. Unsupported or removed names
        // are rejected fail-closed.
        if !crate::tools::is_supported_tool(name) {
            return crate::ai_error_with(
                "UNKNOWN_TOOL",
                &format!("Unknown tool: {name}"),
                Some("Use tools/list to discover supported tools: branch, think, merge."),
                Vec::new(),
            );
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tools::dispatch_tool(self, name, args.clone()).unwrap_or_else(|| {
                crate::ai_error_with(
                    "UNKNOWN_TOOL",
                    &format!("Unknown tool: {name}"),
                    Some("Use tools/list to discover supported tools: branch, think, merge."),
                    Vec::new(),
                )
            })
        }));

        match result {
            Ok(resp) => resp,
            Err(_) => crate::ai_error_with(
                "STORE_ERROR",
                &format!("Internal panic while handling {name}"),
                Some("Retry. If it persists, inspect local logs in the store dir."),
                Vec::new(),
            ),
        }
    }
}
