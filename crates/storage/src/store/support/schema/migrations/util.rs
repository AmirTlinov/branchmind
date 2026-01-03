#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use rusqlite::Connection;

pub(super) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<(), StoreError> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
    match conn.execute(&sql, []) {
        Ok(_) => Ok(()),
        Err(err) if is_duplicate_column(&err) => Ok(()),
        Err(err) => Err(StoreError::Sql(err)),
    }
}

fn is_duplicate_column(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(message)) => {
            message.contains("duplicate column name")
        }
        _ => false,
    }
}
