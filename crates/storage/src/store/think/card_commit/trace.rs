#![forbid(unsafe_code)]

use super::super::super::*;
use rusqlite::{OptionalExtension, Transaction, params};

pub(super) fn insert_trace_entry_if_needed_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    branch: &str,
    trace_doc: &str,
    card: &ThinkCardInput,
    card_id: &str,
    now_ms: i64,
) -> Result<(bool, i64), StoreError> {
    // Trace: idempotent note entry keyed by card_id.
    ensure_document_tx(
        tx,
        workspace,
        branch,
        trace_doc,
        DocumentKind::Trace.as_str(),
        now_ms,
    )?;

    let trace_source_event_id = format!("think_card:{card_id}");
    let existing_row: Option<(i64, Option<String>)> = tx
        .query_row(
            r#"
            SELECT seq, payload_json
            FROM doc_entries
            WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4
            LIMIT 1
            "#,
            params![workspace, branch, trace_doc, trace_source_event_id.as_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    match existing_row {
        Some((seq, Some(existing))) => {
            if existing != card.payload_json {
                return Err(StoreError::InvalidInput(
                    "card_id already exists with a different payload",
                ));
            }
            Ok((false, seq))
        }
        Some((_seq, None)) => Err(StoreError::InvalidInput(
            "card_id already exists but stored payload is missing",
        )),
        None => {
            let inserted_rows = tx.execute(
                r#"
                INSERT OR IGNORE INTO doc_entries(
                  workspace, branch, doc, ts_ms, kind, title, format, meta_json, content,
                  source_event_id, event_type, payload_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
                params![
                    workspace,
                    branch,
                    trace_doc,
                    now_ms,
                    DocEntryKind::Note.as_str(),
                    card.title.as_deref(),
                    "think_card",
                    card.meta_json.as_deref(),
                    card.content.as_str(),
                    trace_source_event_id.as_str(),
                    "think_card",
                    card.payload_json.as_str(),
                ],
            )?;

            let inserted = inserted_rows > 0;
            if inserted {
                touch_document_tx(tx, workspace, branch, trace_doc, now_ms)?;
            }

            let seq: Option<i64> = tx
                .query_row(
                    r#"
                    SELECT seq
                    FROM doc_entries
                    WHERE workspace=?1 AND branch=?2 AND doc=?3 AND source_event_id=?4
                    LIMIT 1
                    "#,
                    params![workspace, branch, trace_doc, trace_source_event_id.as_str()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(seq) = seq else {
                return Err(StoreError::InvalidInput(
                    "trace entry insert succeeded but seq lookup returned none",
                ));
            };

            Ok((inserted, seq))
        }
    }
}
