#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

impl McpServer {
    pub(in super::super) fn resolve_think_commit_scope(
        &mut self,
        workspace: &WorkspaceId,
        args_obj: &serde_json::Map<String, Value>,
    ) -> Result<(String, String, String), Value> {
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch_override = optional_string(args_obj, "branch")?;
        let trace_doc = optional_string(args_obj, "trace_doc")?;
        let graph_doc = optional_string(args_obj, "graph_doc")?;

        ensure_nonempty_doc(&trace_doc, "trace_doc")?;
        ensure_nonempty_doc(&graph_doc, "graph_doc")?;

        let scope = self.resolve_reasoning_scope(
            workspace,
            ReasoningScopeInput {
                target,
                branch: branch_override,
                notes_doc: None,
                graph_doc,
                trace_doc,
            },
        )?;
        Ok((scope.branch, scope.trace_doc, scope.graph_doc))
    }

    pub(in super::super) fn resolve_think_graph_scope(
        &mut self,
        workspace: &WorkspaceId,
        args_obj: &serde_json::Map<String, Value>,
    ) -> Result<(String, String), Value> {
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let reference = optional_string(args_obj, "ref")?;
        let graph_doc = optional_string(args_obj, "graph_doc")?;

        ensure_nonempty_doc(&graph_doc, "graph_doc")?;

        let scope = self.resolve_reasoning_scope(
            workspace,
            ReasoningScopeInput {
                target,
                branch: reference,
                notes_doc: None,
                graph_doc,
                trace_doc: None,
            },
        )?;
        Ok((scope.branch, scope.graph_doc))
    }

    pub(in super::super) fn resolve_think_watch_scope(
        &mut self,
        workspace: &WorkspaceId,
        args_obj: &serde_json::Map<String, Value>,
    ) -> Result<(String, String, String), Value> {
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let reference = optional_string(args_obj, "ref")?;
        let graph_doc = optional_string(args_obj, "graph_doc")?;
        let trace_doc = optional_string(args_obj, "trace_doc")?;

        ensure_nonempty_doc(&graph_doc, "graph_doc")?;
        ensure_nonempty_doc(&trace_doc, "trace_doc")?;

        let scope = self.resolve_reasoning_scope(
            workspace,
            ReasoningScopeInput {
                target,
                branch: reference,
                notes_doc: None,
                graph_doc,
                trace_doc,
            },
        )?;
        Ok((scope.branch, scope.graph_doc, scope.trace_doc))
    }

    pub(in super::super) fn resolve_trace_scope(
        &mut self,
        workspace: &WorkspaceId,
        args_obj: &serde_json::Map<String, Value>,
    ) -> Result<(String, String), Value> {
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let doc = optional_string(args_obj, "doc")?;

        ensure_nonempty_doc(&doc, "doc")?;

        let scope = self.resolve_reasoning_scope(
            workspace,
            ReasoningScopeInput {
                target,
                branch: None,
                notes_doc: None,
                graph_doc: None,
                trace_doc: doc,
            },
        )?;
        Ok((scope.branch, scope.trace_doc))
    }

    pub(in super::super) fn resolve_trace_scope_with_ref(
        &mut self,
        workspace: &WorkspaceId,
        args_obj: &serde_json::Map<String, Value>,
    ) -> Result<(String, String), Value> {
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let reference = optional_string(args_obj, "ref")?;
        let doc = optional_string(args_obj, "doc")?;

        ensure_nonempty_doc(&doc, "doc")?;

        let scope = self.resolve_reasoning_scope(
            workspace,
            ReasoningScopeInput {
                target,
                branch: reference,
                notes_doc: None,
                graph_doc: None,
                trace_doc: doc,
            },
        )?;
        Ok((scope.branch, scope.trace_doc))
    }
}
