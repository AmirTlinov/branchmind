#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use rusqlite::{OptionalExtension, Transaction, params};

const DEFAULT_STEP_LEASE_TTL_SEQ: i64 = 200;
const MAX_STEP_LEASE_TTL_SEQ: i64 = 10_000;

fn current_event_seq_tx(tx: &Transaction<'_>, workspace: &str) -> Result<i64, StoreError> {
    Ok(tx
        .query_row(
            "SELECT seq FROM events WHERE workspace=?1 ORDER BY seq DESC LIMIT 1",
            params![workspace],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn load_step_lease_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    step_id: &str,
) -> Result<Option<(String, i64, i64)>, StoreError> {
    tx.query_row(
        "SELECT holder_agent_id, acquired_seq, expires_seq FROM step_leases WHERE workspace=?1 AND step_id=?2",
        params![workspace, step_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
    )
    .optional()
    .map_err(StoreError::from)
}

pub(super) fn enforce_step_lease_tx(
    tx: &Transaction<'_>,
    workspace: &str,
    step_id: &str,
    agent_id: Option<&str>,
) -> Result<(), StoreError> {
    let Some((holder, _acquired_seq, expires_seq)) = load_step_lease_tx(tx, workspace, step_id)?
    else {
        return Ok(());
    };

    let now_seq = current_event_seq_tx(tx, workspace)?;
    if now_seq >= expires_seq {
        // Expired leases are treated as absent; write ops may GC them.
        tx.execute(
            "DELETE FROM step_leases WHERE workspace=?1 AND step_id=?2",
            params![workspace, step_id],
        )?;
        return Ok(());
    }

    let Some(agent_id) = agent_id else {
        return Err(StoreError::StepLeaseHeld {
            step_id: step_id.to_string(),
            holder_agent_id: holder,
            now_seq,
            expires_seq,
        });
    };
    if holder == agent_id {
        return Ok(());
    }
    Err(StoreError::StepLeaseHeld {
        step_id: step_id.to_string(),
        holder_agent_id: holder,
        now_seq,
        expires_seq,
    })
}

impl SqliteStore {
    pub fn step_lease_get(
        &mut self,
        workspace: &WorkspaceId,
        request: StepLeaseGetRequest,
    ) -> Result<StepLeaseGetResult, StoreError> {
        let StepLeaseGetRequest { task_id, selector } = request;

        let tx = self.conn.transaction()?;
        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.step_id.as_deref(),
            selector.path.as_ref(),
        )?;

        let now_seq = current_event_seq_tx(&tx, workspace.as_str())?;
        let lease = match load_step_lease_tx(&tx, workspace.as_str(), &step_id)? {
            None => None,
            Some((holder, acquired_seq, expires_seq)) if now_seq < expires_seq => Some(StepLease {
                step_id: step_id.clone(),
                holder_agent_id: holder,
                acquired_seq,
                expires_seq,
            }),
            Some(_) => None,
        };

        tx.commit()?;
        Ok(StepLeaseGetResult {
            step: StepRef {
                step_id,
                path: path.clone(),
            },
            lease,
            now_seq,
        })
    }

    pub fn step_lease_claim(
        &mut self,
        workspace: &WorkspaceId,
        request: StepLeaseClaimRequest,
    ) -> Result<StepLeaseOpResult, StoreError> {
        let StepLeaseClaimRequest {
            task_id,
            selector,
            agent_id,
            ttl_seq,
            force,
        } = request;

        let ttl_seq = if ttl_seq <= 0 {
            DEFAULT_STEP_LEASE_TTL_SEQ
        } else {
            ttl_seq
        };
        if ttl_seq > MAX_STEP_LEASE_TTL_SEQ {
            return Err(StoreError::InvalidInput(
                "ttl_seq exceeds max_ttl_seq=10000",
            ));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.step_id.as_deref(),
            selector.path.as_ref(),
        )?;

        let now_seq = current_event_seq_tx(&tx, workspace.as_str())?;
        let mut takeover_from: Option<String> = None;
        if let Some((holder, acquired_seq, expires_seq)) =
            load_step_lease_tx(&tx, workspace.as_str(), &step_id)?
        {
            if now_seq < expires_seq {
                if holder == agent_id {
                    tx.commit()?;
                    return Ok(StepLeaseOpResult {
                        step: StepRef {
                            step_id: step_id.clone(),
                            path: path.clone(),
                        },
                        lease: Some(StepLease {
                            step_id,
                            holder_agent_id: holder,
                            acquired_seq,
                            expires_seq,
                        }),
                        event: None,
                        now_seq,
                    });
                }
                if !force {
                    return Err(StoreError::StepLeaseHeld {
                        step_id,
                        holder_agent_id: holder,
                        now_seq,
                        expires_seq,
                    });
                }
                takeover_from = Some(holder);
            }

            // Expired (or force takeover): treat as a new claim.
            tx.execute(
                "DELETE FROM step_leases WHERE workspace=?1 AND step_id=?2",
                params![workspace.as_str(), step_id],
            )?;
        }

        let event_type = if takeover_from.is_some() {
            "step_lease_taken_over"
        } else {
            "step_lease_claimed"
        };
        let payload_json = serde_json::json!({
            "step_id": step_id,
            "path": path,
            "agent_id": agent_id,
            "ttl_seq": ttl_seq,
            "takeover_from": takeover_from
        })
        .to_string();
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type,
                payload_json: &payload_json,
            },
        )?;

        let expires_seq = event.seq + ttl_seq;
        tx.execute(
            "INSERT OR REPLACE INTO step_leases(workspace, step_id, holder_agent_id, acquired_seq, expires_seq, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                workspace.as_str(),
                step_id,
                agent_id,
                event.seq,
                expires_seq,
                now_ms,
                now_ms
            ],
        )?;

        tx.commit()?;
        let now_seq = event.seq;
        Ok(StepLeaseOpResult {
            step: StepRef {
                step_id: step_id.clone(),
                path: path.clone(),
            },
            lease: Some(StepLease {
                step_id,
                holder_agent_id: agent_id,
                acquired_seq: now_seq,
                expires_seq,
            }),
            event: Some(event),
            now_seq,
        })
    }

    pub fn step_lease_renew(
        &mut self,
        workspace: &WorkspaceId,
        request: StepLeaseRenewRequest,
    ) -> Result<StepLeaseOpResult, StoreError> {
        let StepLeaseRenewRequest {
            task_id,
            selector,
            agent_id,
            ttl_seq,
        } = request;

        let ttl_seq = if ttl_seq <= 0 {
            DEFAULT_STEP_LEASE_TTL_SEQ
        } else {
            ttl_seq
        };
        if ttl_seq > MAX_STEP_LEASE_TTL_SEQ {
            return Err(StoreError::InvalidInput(
                "ttl_seq exceeds max_ttl_seq=10000",
            ));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.step_id.as_deref(),
            selector.path.as_ref(),
        )?;

        let now_seq = current_event_seq_tx(&tx, workspace.as_str())?;
        let lease = load_step_lease_tx(&tx, workspace.as_str(), &step_id)?;
        let Some((holder, _acquired_seq, expires_seq)) = lease else {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: None,
            });
        };
        if now_seq >= expires_seq {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: None,
            });
        }
        if holder != agent_id {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: Some(holder),
            });
        }

        let payload_json = serde_json::json!({
            "step_id": step_id,
            "path": path,
            "agent_id": agent_id,
            "ttl_seq": ttl_seq
        })
        .to_string();
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "step_lease_renewed",
                payload_json: &payload_json,
            },
        )?;

        let expires_seq = event.seq + ttl_seq;
        tx.execute(
            "UPDATE step_leases SET expires_seq=?4, updated_at_ms=?5 WHERE workspace=?1 AND step_id=?2 AND holder_agent_id=?3",
            params![workspace.as_str(), step_id, agent_id, expires_seq, now_ms],
        )?;

        tx.commit()?;
        let now_seq = event.seq;
        Ok(StepLeaseOpResult {
            step: StepRef {
                step_id: step_id.clone(),
                path: path.clone(),
            },
            lease: Some(StepLease {
                step_id,
                holder_agent_id: agent_id,
                acquired_seq: now_seq,
                expires_seq,
            }),
            event: Some(event),
            now_seq,
        })
    }

    pub fn step_lease_release(
        &mut self,
        workspace: &WorkspaceId,
        request: StepLeaseReleaseRequest,
    ) -> Result<StepLeaseOpResult, StoreError> {
        let StepLeaseReleaseRequest {
            task_id,
            selector,
            agent_id,
        } = request;

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let (step_id, path) = resolve_step_selector_tx(
            &tx,
            workspace.as_str(),
            &task_id,
            selector.step_id.as_deref(),
            selector.path.as_ref(),
        )?;

        let now_seq = current_event_seq_tx(&tx, workspace.as_str())?;
        let lease = load_step_lease_tx(&tx, workspace.as_str(), &step_id)?;
        let Some((holder, _acquired_seq, expires_seq)) = lease else {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: None,
            });
        };
        if now_seq >= expires_seq {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: None,
            });
        }
        if holder != agent_id {
            return Err(StoreError::StepLeaseNotHeld {
                step_id,
                holder_agent_id: Some(holder),
            });
        }

        tx.execute(
            "DELETE FROM step_leases WHERE workspace=?1 AND step_id=?2 AND holder_agent_id=?3",
            params![workspace.as_str(), step_id, agent_id],
        )?;

        let payload_json = serde_json::json!({
            "step_id": step_id,
            "path": path,
            "agent_id": agent_id
        })
        .to_string();
        let (event, _reasoning_ref) = emit_task_event_tx(
            &tx,
            TaskEventEmitTxArgs {
                workspace,
                now_ms,
                task_id: &task_id,
                kind: TaskKind::Task,
                path: Some(path.clone()),
                event_type: "step_lease_released",
                payload_json: &payload_json,
            },
        )?;

        tx.commit()?;
        Ok(StepLeaseOpResult {
            step: StepRef { step_id, path },
            lease: None,
            event: Some(event.clone()),
            now_seq: event.seq,
        })
    }
}
