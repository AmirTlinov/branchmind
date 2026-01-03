#![forbid(unsafe_code)]

use super::super::super::StoreError;
use rusqlite::{OptionalExtension, Transaction, params};

pub(in crate::store) fn next_counter_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    name: &str,
) -> Result<i64, StoreError> {
    let current: i64 = tx
        .query_row(
            "SELECT value FROM counters WHERE workspace=?1 AND name=?2",
            params![workspace, name],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let next = current + 1;
    tx.execute(
        r#"
        INSERT INTO counters(workspace, name, value) VALUES (?1, ?2, ?3)
        ON CONFLICT(workspace, name) DO UPDATE SET value=excluded.value
        "#,
        params![workspace, name, next],
    )?;
    Ok(next)
}
