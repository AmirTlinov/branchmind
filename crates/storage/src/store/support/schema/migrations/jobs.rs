#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use super::util::add_column_if_missing;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(conn, "jobs", "claim_expires_at_ms", "INTEGER")?;

    // Best-effort backfill for existing RUNNING jobs so reclaim semantics become explicit after upgrade.
    // Use a conservative default: treat the last runner update as a fresh lease start.
    //
    // Note: this does not infer liveness; it only prevents a null lease from blocking reclaim forever.
    conn.execute(
        r#"
        UPDATE jobs
        SET claim_expires_at_ms = updated_at_ms + 300000
        WHERE status='RUNNING' AND claim_expires_at_ms IS NULL
        "#,
        [],
    )?;

    Ok(())
}
