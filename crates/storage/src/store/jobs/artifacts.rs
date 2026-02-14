#![forbid(unsafe_code)]

use super::*;

impl SqliteStore {
    pub fn job_artifact_create(
        &mut self,
        workspace: &WorkspaceId,
        request: JobArtifactCreateRequest,
    ) -> Result<JobArtifactRow, StoreError> {
        let job_id = normalize_job_id(&request.job_id)?;
        let key = request.artifact_key.trim();
        if key.is_empty() {
            return Err(StoreError::InvalidInput(
                "job_artifact.artifact_key must not be empty",
            ));
        }
        if key.len() > MAX_ARTIFACT_KEY_LEN {
            return Err(StoreError::InvalidInput(
                "job_artifact.artifact_key is too long",
            ));
        }
        let content = &request.content_text;
        let content_len = content.len();
        if content_len > MAX_JOB_ARTIFACT_LEN {
            return Err(StoreError::InvalidInput(
                "job_artifact.content_text exceeds max length (512KB)",
            ));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        // Verify job exists.
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM jobs WHERE workspace=?1 AND id=?2",
                params![workspace.as_str(), job_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::UnknownId);
        }

        // Check artifact count limit.
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM job_artifacts WHERE workspace=?1 AND job_id=?2",
            params![workspace.as_str(), job_id.as_str()],
            |row| row.get(0),
        )?;
        if count as usize >= MAX_ARTIFACTS_PER_JOB {
            // Check if this is an upsert (key already exists).
            let key_exists: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM job_artifacts WHERE workspace=?1 AND job_id=?2 AND artifact_key=?3",
                    params![workspace.as_str(), job_id.as_str(), key],
                    |row| row.get(0),
                )
                .optional()?;
            if key_exists.is_none() {
                return Err(StoreError::InvalidInput(
                    "job_artifact: max artifacts per job exceeded (8)",
                ));
            }
        }

        tx.execute(
            r#"
            INSERT INTO job_artifacts(workspace, job_id, artifact_key, content_text, content_len, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(workspace, job_id, artifact_key) DO UPDATE
              SET content_text=excluded.content_text, content_len=excluded.content_len, created_at_ms=excluded.created_at_ms
            "#,
            params![
                workspace.as_str(),
                job_id.as_str(),
                key,
                content,
                content_len as i64,
                now_ms
            ],
        )?;

        tx.commit()?;

        Ok(JobArtifactRow {
            job_id,
            artifact_key: key.to_string(),
            content_text: content.clone(),
            content_len: content_len as i64,
            created_at_ms: now_ms,
        })
    }

    pub fn job_artifact_get(
        &mut self,
        workspace: &WorkspaceId,
        request: JobArtifactGetRequest,
    ) -> Result<Option<JobArtifactRow>, StoreError> {
        let job_id = normalize_job_id(&request.job_id)?;
        let key = request.artifact_key.trim();
        if key.is_empty() {
            return Err(StoreError::InvalidInput(
                "job_artifact.artifact_key must not be empty",
            ));
        }

        let row: Option<(String, i64, i64)> = self
            .conn
            .query_row(
                r#"
                SELECT content_text, content_len, created_at_ms
                FROM job_artifacts
                WHERE workspace=?1 AND job_id=?2 AND artifact_key=?3
                "#,
                params![workspace.as_str(), job_id.as_str(), key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        Ok(row.map(
            |(content_text, content_len, created_at_ms)| JobArtifactRow {
                job_id,
                artifact_key: key.to_string(),
                content_text,
                content_len,
                created_at_ms,
            },
        ))
    }

    pub fn job_artifacts_list(
        &mut self,
        workspace: &WorkspaceId,
        request: JobArtifactsListRequest,
    ) -> Result<Vec<JobArtifactMetaRow>, StoreError> {
        let job_id = normalize_job_id(&request.job_id)?;
        let limit = request.limit.max(1);

        let mut stmt = self.conn.prepare(
            r#"
            SELECT artifact_key, content_len, created_at_ms
            FROM job_artifacts
            WHERE workspace=?1 AND job_id=?2
            ORDER BY artifact_key ASC
            LIMIT ?3
            "#,
        )?;

        let mut rows = stmt.query(params![workspace.as_str(), job_id.as_str(), limit])?;
        let mut out = Vec::<JobArtifactMetaRow>::new();
        while let Some(row) = rows.next()? {
            let artifact_key: String = row.get(0)?;
            let content_len: i64 = row.get(1)?;
            let created_at_ms: i64 = row.get(2)?;
            out.push(JobArtifactMetaRow {
                job_id: job_id.clone(),
                artifact_key,
                content_len,
                created_at_ms,
            });
        }
        Ok(out)
    }
}
