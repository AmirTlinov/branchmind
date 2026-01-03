#![forbid(unsafe_code)]

mod migrations;
mod sql;

use super::super::StoreError;
use rusqlite::{Connection, params};

pub(in crate::store) fn migrate_sqlite_schema(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(&sql::full_schema_sql())?;

    migrations::apply(conn)?;

    conn.execute(
        "INSERT OR IGNORE INTO meta(key, value) VALUES (?1, ?2)",
        params!["schema_version", "v0"],
    )?;

    Ok(())
}
