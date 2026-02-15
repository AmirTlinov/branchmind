#![forbid(unsafe_code)]

use super::super::super::super::StoreError;
use rusqlite::Connection;

pub(super) fn apply(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        r#"
            DROP INDEX IF EXISTS idx_knowledge_keys_anchor_updated;
            DROP INDEX IF EXISTS idx_knowledge_keys_workspace_updated;
            DROP INDEX IF EXISTS idx_knowledge_keys_key_updated;
            DROP INDEX IF EXISTS idx_knowledge_keys_card;
            DROP TABLE IF EXISTS knowledge_keys;
        "#,
    )?;
    Ok(())
}
