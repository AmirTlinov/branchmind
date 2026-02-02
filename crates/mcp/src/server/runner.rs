#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;

impl McpServer {
    pub(crate) fn runner_bootstrap_json(&self, workspace: &crate::WorkspaceId) -> Value {
        let storage_dir = self.store.storage_dir();
        let storage_dir =
            std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
        let mcp_bin =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("bm_mcp"));
        let runner_bin = mcp_bin
            .parent()
            .map(|dir| dir.join("bm_runner"))
            .filter(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("bm_runner"));

        let cmd = format!(
            "\"{}\" --storage-dir \"{}\" --workspace \"{}\" --mcp-bin \"{}\"",
            runner_bin.to_string_lossy(),
            storage_dir.to_string_lossy(),
            workspace.as_str(),
            mcp_bin.to_string_lossy()
        );

        json!({
            "cmd": cmd,
            "runner_bin": runner_bin.to_string_lossy(),
            "mcp_bin": mcp_bin.to_string_lossy(),
            "storage_dir": storage_dir.to_string_lossy()
        })
    }

    pub(crate) fn start_runner_on_demand(
        &mut self,
        workspace: &crate::WorkspaceId,
        now_ms: i64,
    ) -> std::io::Result<bool> {
        let key = workspace.as_str().to_string();

        {
            let mut state = self
                .runner_autostart
                .lock()
                .expect("runner_autostart mutex poisoned");
            let entry =
                state
                    .entries
                    .entry(key.clone())
                    .or_insert_with(|| crate::RunnerAutostartEntry {
                        last_attempt_ms: 0,
                        last_attempt_ok: false,
                        child: None,
                    });

            // Reap finished children to avoid zombies. If still running, treat as started.
            if let Some(child) = entry.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(_)) => entry.child = None,
                    Ok(None) => return Ok(true),
                    Err(_) => entry.child = None,
                }
            }
        }

        let spawn_result = self.spawn_runner_for_autostart(workspace);
        let mut state = self
            .runner_autostart
            .lock()
            .expect("runner_autostart mutex poisoned");
        let entry = state
            .entries
            .entry(key)
            .or_insert_with(|| crate::RunnerAutostartEntry {
                last_attempt_ms: 0,
                last_attempt_ok: false,
                child: None,
            });

        entry.last_attempt_ms = now_ms;
        match spawn_result {
            Ok(child) => {
                entry.child = Some(child);
                entry.last_attempt_ok = true;
                Ok(true)
            }
            Err(err) => {
                entry.last_attempt_ok = false;
                entry.child = None;
                Err(err)
            }
        }
    }

    pub(crate) fn maybe_autostart_runner(
        &mut self,
        workspace: &crate::WorkspaceId,
        now_ms: i64,
        queued_jobs: usize,
        runner_is_offline: bool,
    ) -> bool {
        if !self.runner_autostart_enabled.load(Ordering::Relaxed) {
            return false;
        }
        if queued_jobs == 0 || !runner_is_offline {
            return false;
        }

        // Per-workspace throttle: avoid spawning on every portal refresh.
        let key = workspace.as_str().to_string();
        {
            let mut state = self
                .runner_autostart
                .lock()
                .expect("runner_autostart mutex poisoned");
            let entry =
                state
                    .entries
                    .entry(key.clone())
                    .or_insert_with(|| crate::RunnerAutostartEntry {
                        last_attempt_ms: 0,
                        last_attempt_ok: false,
                        child: None,
                    });

            // Reap finished children to avoid zombies.
            if let Some(child) = entry.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(_)) => entry.child = None,
                    Ok(None) => return true, // still running
                    Err(_) => entry.child = None,
                }
            }

            const BACKOFF_MS: i64 = 30_000;
            if now_ms.saturating_sub(entry.last_attempt_ms) < BACKOFF_MS {
                return entry.last_attempt_ok;
            }
        }

        let spawn_result = self.spawn_runner_for_autostart(workspace);
        let mut state = self
            .runner_autostart
            .lock()
            .expect("runner_autostart mutex poisoned");
        let entry = state
            .entries
            .get_mut(&key)
            .expect("runner_autostart entry must exist");
        entry.last_attempt_ms = now_ms;
        match spawn_result {
            Ok(child) => {
                entry.child = Some(child);
                entry.last_attempt_ok = true;
                true
            }
            Err(_) => {
                entry.last_attempt_ok = false;
                false
            }
        }
    }

    fn spawn_runner_for_autostart(
        &self,
        workspace: &crate::WorkspaceId,
    ) -> std::io::Result<std::process::Child> {
        let storage_dir = self.store.storage_dir();
        let storage_dir =
            std::fs::canonicalize(storage_dir).unwrap_or_else(|_| storage_dir.to_path_buf());
        let mcp_bin =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("bm_mcp"));
        let runner_bin = mcp_bin
            .parent()
            .map(|dir| dir.join("bm_runner"))
            .filter(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("bm_runner"));

        let mut cmd = Command::new(runner_bin);
        cmd.arg("--storage-dir")
            .arg(storage_dir)
            .arg("--workspace")
            .arg(workspace.as_str())
            .arg("--mcp-bin")
            .arg(mcp_bin);

        if self.runner_autostart_dry_run {
            cmd.arg("--dry-run").arg("--once");
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.spawn()
    }

    pub(crate) fn tool_storage(&mut self, _args: Value) -> Value {
        crate::ai_ok(
            "storage",
            json!( {
                "storage_dir": self.store.storage_dir().to_string_lossy().to_string(),
            }),
        )
    }
}
