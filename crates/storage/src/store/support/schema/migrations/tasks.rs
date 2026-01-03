#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use super::util::add_column_if_missing;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(conn, "tasks", "status", "TEXT NOT NULL DEFAULT 'TODO'")?;
    add_column_if_missing(conn, "tasks", "status_manual", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "tasks", "priority", "TEXT NOT NULL DEFAULT 'MEDIUM'")?;
    add_column_if_missing(conn, "tasks", "blocked", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "tasks", "assignee", "TEXT")?;
    add_column_if_missing(conn, "tasks", "domain", "TEXT")?;
    add_column_if_missing(conn, "tasks", "phase", "TEXT")?;
    add_column_if_missing(conn, "tasks", "component", "TEXT")?;
    add_column_if_missing(conn, "tasks", "context", "TEXT")?;
    add_column_if_missing(
        conn,
        "tasks",
        "criteria_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "tests_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "criteria_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "tests_auto_confirmed",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "security_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "perf_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "tasks",
        "docs_confirmed",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    Ok(())
}
