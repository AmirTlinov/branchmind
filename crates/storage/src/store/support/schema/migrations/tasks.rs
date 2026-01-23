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
    add_column_if_missing(conn, "tasks", "parked_until_ts_ms", "INTEGER")?;
    add_column_if_missing(conn, "tasks", "stale_after_ms", "INTEGER")?;
    add_column_if_missing(
        conn,
        "tasks",
        "reasoning_mode",
        "TEXT NOT NULL DEFAULT 'normal'",
    )?;
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

    // Indexes for horizons (parked/stale/next_wake). Kept in migrations so existing DBs get
    // columns before index creation (schema SQL runs before migrations).
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_tasks_plan_status_updated
          ON tasks(workspace, parent_plan_id, status, updated_at_ms, id);
        CREATE INDEX IF NOT EXISTS idx_tasks_plan_status_parked_until
          ON tasks(workspace, parent_plan_id, status, parked_until_ts_ms, id);
        "#,
    )?;

    Ok(())
}
