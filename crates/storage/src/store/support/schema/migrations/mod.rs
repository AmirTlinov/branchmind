#![forbid(unsafe_code)]

mod plans;
mod steps;
mod tasks;
mod util;

use super::super::super::StoreError;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    plans::apply(conn)?;
    tasks::apply(conn)?;
    steps::apply(conn)?;
    Ok(())
}
