#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

fn mermaid_id(anchor_id: &str) -> String {
    let mut out = String::new();
    out.push('A');
    out.push('_');
    for ch in anchor_id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

fn export_mermaid(anchors: &[bm_storage::AnchorRow]) -> String {
    let mut out = String::new();
    out.push_str("flowchart TD\n");

    for a in anchors {
        let id = mermaid_id(&a.id);
        let label = format!("{}\\n{}", a.id, a.title);
        out.push_str(&format!("  {id}[\"{label}\"]\n"));
    }

    for a in anchors {
        if let Some(parent) = a.parent_id.as_ref() {
            out.push_str(&format!(
                "  {} -->|contains| {}\n",
                mermaid_id(parent),
                mermaid_id(&a.id)
            ));
        }
        for dep in &a.depends_on {
            out.push_str(&format!(
                "  {} -->|depends_on| {}\n",
                mermaid_id(&a.id),
                mermaid_id(dep)
            ));
        }
    }

    out
}

fn export_text(anchors: &[bm_storage::AnchorRow]) -> String {
    let mut out = String::new();
    for a in anchors {
        let parent = a.parent_id.as_deref().unwrap_or("-");
        let depends = if a.depends_on.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", a.depends_on.join(","))
        };
        out.push_str(&format!(
            "{} | kind={} status={} parent={} depends_on={}\n",
            a.id, a.kind, a.status, parent, depends
        ));
    }
    out
}

impl McpServer {
    pub(crate) fn tool_branchmind_anchors_export(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let format = match optional_string(args_obj, "format") {
            Ok(v) => v.unwrap_or_else(|| "mermaid".to_string()),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let mut anchors = match self.store.anchors_list(
            &workspace,
            bm_storage::AnchorsListRequest {
                text: None,
                kind: None,
                status: None,
                limit: 200,
            },
        ) {
            Ok(v) => v.anchors,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        anchors.sort_by(|a, b| a.id.cmp(&b.id));

        let rendered = match format.trim().to_ascii_lowercase().as_str() {
            "mermaid" => export_mermaid(&anchors),
            "text" => export_text(&anchors),
            _ => return ai_error("INVALID_INPUT", "format must be one of: mermaid, text"),
        };

        let mut out = rendered;
        let mut truncated = false;
        if let Some(limit) = max_chars
            && limit > 0
            && out.chars().count() > limit
        {
            out = out
                .chars()
                .take(limit.saturating_sub(1))
                .collect::<String>();
            out.push('…');
            truncated = true;
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "format": format,
            "anchors_count": anchors.len(),
            "text": out,
            "truncated": truncated
        });

        if truncated {
            let warnings = budget_warnings(true, false, false);
            ai_ok_with_warnings("anchors_export", result, warnings, Vec::new())
        } else {
            ai_ok("anchors_export", result)
        }
    }
}
