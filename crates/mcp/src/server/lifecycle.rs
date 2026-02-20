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
            ux_proof_v2_enabled: cfg.ux_proof_v2_enabled,
            jobs_unknown_args_fail_closed_enabled: cfg.jobs_unknown_args_fail_closed_enabled,
            jobs_strict_progress_schema_enabled: cfg.jobs_strict_progress_schema_enabled,
            jobs_high_done_proof_gate_enabled: cfg.jobs_high_done_proof_gate_enabled,
            jobs_wait_stream_v2_enabled: cfg.jobs_wait_stream_v2_enabled,
            jobs_mesh_v1_enabled: cfg.jobs_mesh_v1_enabled,
            slice_plans_v1_enabled: cfg.slice_plans_v1_enabled,
            jobs_slice_first_fail_closed_enabled: cfg.jobs_slice_first_fail_closed_enabled,
            slice_budgets_enforced_enabled: cfg.slice_budgets_enforced_enabled,
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
            // v3: strict surface = 3 markdown tools; toolset params are ignored.
            let tools = crate::tools_v1::tool_definitions();
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
        let raw_name = name.to_string();
        let name_norm = normalize_tool_name(&raw_name);
        let name_ref = name_norm;

        // v3 cutover: only the 3 advertised markdown tools are callable.
        // Legacy names are rejected fail-closed.
        if !crate::tools_v1::is_v1_tool(name_ref) {
            return crate::ops::error_unknown_tool(name_ref);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Keep the existing server pipeline (workspace guards, budget discipline, etc.)
            // to avoid dead subsystems and to preserve deterministic runtime behavior.
            //
            // v1 tools return v1-shaped responses; pipeline pre/post hooks still apply.
            let mut args_mut = args.clone();
            if let Some(pre) = self.preprocess_args(name_ref, &mut args_mut) {
                let ws = args_mut.get("workspace").and_then(|v| v.as_str());
                let mut resp = crate::ops::handler_to_op_response(name_ref, ws, pre).into_value();
                self.postprocess_response(name_ref, &args_mut, &mut resp);
                return resp;
            }

            let mut resp = crate::tools_v1::dispatch_tool(self, name_ref, args_mut.clone())
                .unwrap_or_else(|| crate::ops::error_unknown_tool(name_ref));
            self.postprocess_response(name_ref, &args_mut, &mut resp);
            resp
        }));

        match result {
            Ok(resp) => resp,
            Err(_) => {
                crate::ops::error_internal(format!("Internal panic while handling {name_ref}"))
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
    if let Some((prefix, suffix)) = name.split_once('.')
        && prefix == "bm"
    {
        return suffix;
    }
    name
}
