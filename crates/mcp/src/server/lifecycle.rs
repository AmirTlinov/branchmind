#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn new(store: bm_storage::SqliteStore, cfg: crate::McpServerConfig) -> Self {
        Self {
            initialized: false,
            store,
            toolset: cfg.toolset,
            default_workspace: cfg.default_workspace,
            workspace_explicit: cfg.workspace_explicit,
            workspace_override: None,
            workspace_allowlist: cfg.workspace_allowlist,
            workspace_lock: cfg.workspace_lock,
            project_guard: cfg.project_guard,
            project_guard_rebind_enabled: cfg.project_guard_rebind_enabled,
            default_agent_id: cfg.default_agent_id,
            runner_autostart_enabled: cfg.runner_autostart_enabled,
            runner_autostart_dry_run: cfg.runner_autostart_dry_run,
            runner_autostart: cfg.runner_autostart,
        }
    }

    pub(crate) fn handle(&mut self, request: crate::JsonRpcRequest) -> Option<Value> {
        let method = request.method.as_str();

        if method == "initialize" {
            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "protocolVersion": crate::MCP_VERSION,
                    "serverInfo": {
                        "name": crate::SERVER_NAME,
                        "version": crate::build_fingerprint()
                    },
                    "capabilities": { "tools": {} }
                }),
            ));
        }

        if method == "notifications/initialized" {
            self.initialized = true;
            return None;
        }

        if !self.initialized {
            // Out-of-box DX: allow auto-initialization on first real request. This avoids
            // client startup races that would otherwise yield "Server not initialized".
            if matches!(
                method,
                "tools/call" | "tools/list" | "resources/list" | "resources/read" | "ping"
            ) {
                self.initialized = true;
            } else {
                return Some(crate::json_rpc_error(
                    request.id,
                    -32002,
                    "Server not initialized",
                ));
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
        if method == "resources/read" {
            return Some(crate::json_rpc_response(
                request.id,
                json!({ "contents": [] }),
            ));
        }

        if method == "tools/list" {
            let toolset = match request
                .params
                .as_ref()
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get("toolset"))
            {
                Some(v) => {
                    let Some(label) = v.as_str() else {
                        return Some(crate::json_rpc_error(
                            request.id,
                            -32602,
                            "toolset must be a string",
                        ));
                    };
                    match crate::Toolset::from_str(label) {
                        Some(v) => v,
                        None => {
                            return Some(crate::json_rpc_error(
                                request.id,
                                -32602,
                                "toolset must be one of: full|daily|core",
                            ));
                        }
                    }
                }
                None => self.toolset,
            };
            let mut tools = crate::tools::tool_definitions(toolset);
            // Flagship DX: when the server is configured with a default workspace, treat
            // `workspace` as optional in tool schemas so agents aren't forced to pick it.
            // (Explicit workspace is still supported and may be locked by workspace_lock.)
            if self.default_workspace.is_some() {
                for tool in &mut tools {
                    let Some(schema_obj) =
                        tool.get_mut("inputSchema").and_then(|v| v.as_object_mut())
                    else {
                        continue;
                    };
                    let Some(required) = schema_obj
                        .get_mut("required")
                        .and_then(|v| v.as_array_mut())
                    else {
                        continue;
                    };
                    required.retain(|v| v.as_str() != Some("workspace"));
                }
            }
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
            let args = params_obj
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response_body = self.call_tool(tool_name, args);

            return Some(crate::json_rpc_response(
                request.id,
                json!( {
                    "content": [crate::tool_text_content(&response_body)],
                    "isError": !response_body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
                }),
            ));
        }

        Some(crate::json_rpc_error(
            request.id,
            -32601,
            &format!("Method not found: {method}"),
        ))
    }

    pub(crate) fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let mut args = args;
        let original_args = args.clone();
        if let Some(mut resp) = self.preprocess_args(name, &mut args) {
            // Even when we short-circuit during preprocessing (e.g. target normalization errors),
            // we still want the same portal-first recovery UX and the same output formatting.
            self.postprocess_response(name, &args, &mut resp);
            return resp;
        }
        let Some(mut resp) = crate::tools::dispatch_tool(self, name, args.clone()) else {
            let mut resp = crate::ai_error("UNKNOWN_TOOL", &format!("Unknown tool: {name}"));
            self.postprocess_response(name, &args, &mut resp);
            return resp;
        };
        if let Some((escalated_args, mut escalated_resp)) =
            self.auto_escalate_budget_if_needed(name, &original_args, &args, &resp)
        {
            self.postprocess_response(name, &escalated_args, &mut escalated_resp);
            return escalated_resp;
        }
        self.postprocess_response(name, &args, &mut resp);
        resp
    }
}
