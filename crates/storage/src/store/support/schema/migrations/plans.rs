#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use super::util::add_column_if_missing;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(conn, "plans", "description", "TEXT")?;
    add_column_if_missing(conn, "plans", "context", "TEXT")?;
    add_column_if_missing(conn, "plans", "status", "TEXT NOT NULL DEFAULT 'TODO'")?;
    add_column_if_missing(conn, "plans", "status_manual", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "plans", "priority", "TEXT NOT NULL DEFAULT 'MEDIUM'")?;
    add_column_if_missing(conn, "plans", "plan_doc", "TEXT")?;
    add_column_if_missing(conn, "plans", "plan_current", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(
        conn,
        "plans",
        "criteria_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "tests_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "criteria_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "tests_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "security_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "perf_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "plans",
        "docs_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    Ok(())
}
