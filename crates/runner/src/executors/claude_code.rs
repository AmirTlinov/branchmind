#![forbid(unsafe_code)]

use crate::RunnerConfig;
use serde_json::Value;
use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub(crate) fn spawn_exec(
    cfg: &RunnerConfig,
    schema_json: &str,
    out_path: &Path,
    stderr_path: &Path,
    prompt: &str,
    model: Option<&str>,
) -> Result<Child, String> {
    let Some(claude_bin) = cfg.claude_bin.as_deref() else {
        return Err(
            "claude_code executor is not configured (set --claude-bin or BM_CLAUDE_BIN)"
                .to_string(),
        );
    };

    let stdout_file =
        File::create(out_path).map_err(|e| format!("create claude stdout capture failed: {e}"))?;
    let stderr_file = File::create(stderr_path)
        .map_err(|e| format!("create claude stderr capture failed: {e}"))?;

    let mut cmd = Command::new(claude_bin);
    cmd.arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(schema_json)
        .arg("--no-session-persistence")
        // Flagship DX: avoid interactive permission prompts (runner must never hang).
        .arg("--dangerously-skip-permissions")
        // Keep tool execution rooted in the repo.
        .arg("--add-dir")
        .arg(cfg.repo_root.to_string_lossy().to_string());

    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }

    // `claude` does not accept a `--cwd` flag (as of 2.x); set the process working directory
    // directly to keep file operations deterministic and within the repo.
    cmd.current_dir(&cfg.repo_root);

    let child = cmd
        .arg(prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("failed to spawn claude ({claude_bin}): {e}"))?;

    Ok(child)
}

pub(crate) fn read_output(out_path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(out_path).map_err(|e| format!("read claude output failed: {e}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("parse claude json failed: {e}"))?;

    // Claude Code `--output-format json` returns a wrapper object with metadata and the schema-
    // validated payload nested under `structured_output`.
    //
    // Example (abridged):
    // {
    //   "type":"result",
    //   ...,
    //   "structured_output": { "status":"DONE", "summary":"...", "refs":[...], "events":[...] }
    // }
    //
    // The runner contract expects the structured output object directly, so unwrap when present.
    if let Some(structured) = value.get("structured_output") {
        return Ok(structured.clone());
    }

    Ok(value)
}
