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

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS plan_slices (
          workspace TEXT NOT NULL,
          plan_id TEXT NOT NULL,
          slice_id TEXT NOT NULL,
          slice_task_id TEXT NOT NULL,
          title TEXT NOT NULL,
          objective TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'planned',
          budgets_json TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, plan_id, slice_id)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_plan_slices_workspace_slice ON plan_slices(workspace, slice_id);
        CREATE INDEX IF NOT EXISTS idx_plan_slices_plan_updated ON plan_slices(workspace, plan_id, updated_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_plan_slices_task ON plan_slices(workspace, slice_task_id);
        "#,
    )?;

    Ok(())
}
