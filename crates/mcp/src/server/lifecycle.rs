#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn new(store: bm_storage::SqliteStore, cfg: crate::McpServerConfig) -> Self {
        Self {
            initialized: false,
            store,
            toolset: cfg.toolset,
            response_verbosity: cfg.response_verbosity,
            dx_mode: cfg.dx_mode,
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
        let raw_name = name.to_string();
        let name_norm = normalize_tool_name(&raw_name);
        let mut args = args;
        let original_args = args.clone();
        let name_ref = name_norm;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(mut resp) = self.preprocess_args(name_ref, &mut args) {
                // Even when we short-circuit during preprocessing (e.g. target normalization errors),
                // we still want the same portal-first recovery UX and the same output formatting.
                self.postprocess_response(name_ref, &args, &mut resp);
                return resp;
            }
            let Some(mut resp) = crate::tools::dispatch_tool(self, name_ref, args.clone()) else {
                let mut resp =
                    crate::ai_error("UNKNOWN_TOOL", &format!("Unknown tool: {raw_name}"));
                self.postprocess_response(name_ref, &args, &mut resp);
                return resp;
            };
            if let Some((escalated_args, mut escalated_resp)) =
                self.auto_escalate_budget_if_needed(name_ref, &original_args, &args, &resp)
            {
                self.postprocess_response(name_ref, &escalated_args, &mut escalated_resp);
                return escalated_resp;
            }
            self.postprocess_response(name_ref, &args, &mut resp);
            resp
        }));

        match result {
            Ok(resp) => resp,
            Err(_) => {
                let mut resp = crate::ai_error_with(
                    "INTERNAL_PANIC",
                    &format!("Internal panic while handling {name_ref}"),
                    Some("Retry the call. If it repeats, restart the server and capture logs."),
                    Vec::new(),
                );
                let mut post_args = original_args.clone();
                if super::portal::is_portal_tool(name_ref)
                    && let Some(obj) = post_args.as_object_mut()
                {
                    obj.entry("fmt".to_string())
                        .or_insert(Value::String("lines".to_string()));
                }
                self.postprocess_response(name_ref, &post_args, &mut resp);
                resp
            }
        }
    }
}

fn normalize_tool_name(name: &str) -> &str {
    // MCP client interoperability:
    // Some clients incorrectly include the server namespace in the tool name, e.g.:
    // - "branchmind/status" instead of "status"
    // - "branchmind.status" instead of "status"
    //
    // The MCP server namespace is already provided by the transport (server selection),
    // so we accept these variants to avoid spurious "unknown tool" failures.
    let name = name.trim();
    if let Some((_, suffix)) = name.rsplit_once('/') {
        return suffix;
    }
    if let Some((prefix, suffix)) = name.split_once('.')
        && prefix == "branchmind"
    {
        return suffix;
    }
    name
}
