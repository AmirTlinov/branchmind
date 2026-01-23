#![forbid(unsafe_code)]

mod anchors;
mod jobs;
mod plans;
mod steps;
mod tasks;
mod util;
mod workspaces;

use super::super::super::StoreError;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    workspaces::apply(conn)?;
    plans::apply(conn)?;
    tasks::apply(conn)?;
    steps::apply(conn)?;
    jobs::apply(conn)?;
    anchors::apply(conn)?;
    Ok(())
}
