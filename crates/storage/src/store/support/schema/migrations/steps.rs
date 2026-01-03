#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use super::util::add_column_if_missing;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(conn, "steps", "completed_at_ms", "INTEGER")?;
    add_column_if_missing(conn, "steps", "started_at_ms", "INTEGER")?;
    add_column_if_missing(
        conn,
        "steps",
        "criteria_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "tests_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "security_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "perf_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "docs_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "steps", "blocked", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "steps", "block_reason", "TEXT")?;
    add_column_if_missing(conn, "steps", "verification_outcome", "TEXT")?;
    add_column_if_missing(
        conn,
        "steps",
        "proof_tests_mode",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "proof_security_mode",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "proof_perf_mode",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "steps",
        "proof_docs_mode",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    Ok(())
}
