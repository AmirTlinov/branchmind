#![forbid(unsafe_code)]

use super::super::super::{DocEntryKind, GraphOp, StoreError};
use super::super::{json_escape, looks_like_json_object};
use rusqlite::{Transaction, params};

pub(in crate::store) fn build_graph_op_event(op: &GraphOp) -> (&'static str, String) {
    fn push_opt_str(out: &mut String, key: &str, value: Option<&str>) {
        let Some(value) = value else {
            return;
        };
        out.push_str(",\"");
        out.push_str(key);
        out.push_str("\":\"");
        out.push_str(&json_escape(value));
        out.push('"');
    }

    fn push_opt_meta(out: &mut String, meta_json: Option<&str>) {
        let Some(meta_json) = meta_json else {
            return;
        };
        let trimmed = meta_json.trim();
        if looks_like_json_object(trimmed) {
            out.push_str(",\"meta\":");
            out.push_str(trimmed);
        } else {
            out.push_str(",\"meta_raw\":\"");
            out.push_str(&json_escape(trimmed));
            out.push('"');
        }
    }

    fn push_tags(out: &mut String, tags: &[String]) {
        if tags.is_empty() {
            return;
        }
        out.push_str(",\"tags\":[");
        for (i, tag) in tags.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&json_escape(tag));
            out.push('"');
        }
        out.push(']');
    }

    match op {
        GraphOp::NodeUpsert(upsert) => {
            let mut out = String::new();
            out.push_str("{\"op\":\"node_upsert\",\"id\":\"");
            out.push_str(&json_escape(&upsert.id));
            out.push_str("\",\"type\":\"");
            out.push_str(&json_escape(&upsert.node_type));
            out.push('"');
            push_opt_str(&mut out, "title", upsert.title.as_deref());
            push_opt_str(&mut out, "text", upsert.text.as_deref());
            push_opt_str(&mut out, "status", upsert.status.as_deref());
            push_tags(&mut out, &upsert.tags);
            push_opt_meta(&mut out, upsert.meta_json.as_deref());
            out.push('}');
            ("graph_node_upsert", out)
        }
        GraphOp::NodeDelete { id } => (
            "graph_node_delete",
            format!("{{\"op\":\"node_delete\",\"id\":\"{}\"}}", json_escape(id)),
        ),
        GraphOp::EdgeUpsert(upsert) => {
            let mut out = String::new();
            out.push_str("{\"op\":\"edge_upsert\",\"from\":\"");
            out.push_str(&json_escape(&upsert.from));
            out.push_str("\",\"rel\":\"");
            out.push_str(&json_escape(&upsert.rel));
            out.push_str("\",\"to\":\"");
            out.push_str(&json_escape(&upsert.to));
            out.push('"');
            push_opt_meta(&mut out, upsert.meta_json.as_deref());
            out.push('}');
            ("graph_edge_upsert", out)
        }
        GraphOp::EdgeDelete { from, rel, to } => (
            "graph_edge_delete",
            format!(
                "{{\"op\":\"edge_delete\",\"from\":\"{}\",\"rel\":\"{}\",\"to\":\"{}\"}}",
                json_escape(from),
                json_escape(rel),
                json_escape(to)
            ),
        ),
    }
}

pub(in crate::store) fn insert_graph_doc_entry_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    doc: &str,
    ts_ms: i64,
    op: &GraphOp,
    source_event_id: Option<&str>,
) -> Result<(String, Option<i64>), StoreError> {
    let (event_type, payload_json) = build_graph_op_event(op);
    let inserted = tx.execute(
        r#"
        INSERT OR IGNORE INTO doc_entries(workspace, branch, doc, ts_ms, kind, source_event_id, event_type, payload_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            workspace,
            branch,
            doc,
            ts_ms,
            DocEntryKind::Event.as_str(),
            source_event_id,
            event_type,
            &payload_json
        ],
    )?;

    if inserted > 0 {
        Ok((payload_json, Some(tx.last_insert_rowid())))
    } else {
        Ok((payload_json, None))
    }
}
