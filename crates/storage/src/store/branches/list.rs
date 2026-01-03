use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn branch_list(
        &self,
        workspace: &WorkspaceId,
        limit: usize,
    ) -> Result<Vec<BranchInfo>, StoreError> {
        use std::collections::HashMap;

        let limit = limit.clamp(1, 500);
        let mut map: HashMap<String, BranchInfo> = HashMap::new();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT name, base_branch, base_seq, created_at_ms
            FROM branches
            WHERE workspace=?1
            ORDER BY name ASC
            "#,
        )?;
        let rows = stmt.query_map(params![workspace.as_str()], |row| {
            Ok(BranchInfo {
                name: row.get::<_, String>(0)?,
                base_branch: Some(row.get::<_, String>(1)?),
                base_seq: Some(row.get::<_, i64>(2)?),
                created_at_ms: Some(row.get::<_, i64>(3)?),
            })
        })?;
        for row in rows {
            let info = row?;
            map.insert(info.name.clone(), info);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM reasoning_refs WHERE workspace=?1")?;
        let refs = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in refs {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT branch FROM doc_entries WHERE workspace=?1")?;
        let entries = stmt.query_map(params![workspace.as_str()], |row| row.get::<_, String>(0))?;
        for branch in entries {
            let branch = branch?;
            map.entry(branch.clone()).or_insert(BranchInfo {
                name: branch,
                base_branch: None,
                base_seq: None,
                created_at_ms: None,
            });
        }

        let mut names = map.keys().cloned().collect::<Vec<_>>();
        names.sort();
        let mut out = Vec::new();
        for name in names.into_iter().take(limit) {
            if let Some(info) = map.remove(&name) {
                out.push(info);
            }
        }
        Ok(out)
    }
}
