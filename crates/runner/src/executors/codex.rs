#![forbid(unsafe_code)]

use crate::RunnerConfig;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub(crate) fn spawn_exec(
    cfg: &RunnerConfig,
    schema_path: &Path,
    out_path: &Path,
    stderr_path: &Path,
    prompt: &str,
) -> Result<Child, String> {
    let stderr_file = File::create(stderr_path)
        .map_err(|e| format!("create codex stderr capture failed: {e}"))?;

    let mut child = Command::new(&cfg.codex_bin)
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg("approval_policy=\"never\"")
        .arg("-s")
        .arg("workspace-write")
        .arg("--output-schema")
        .arg(schema_path)
        .arg("--output-last-message")
        .arg(out_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("failed to spawn codex exec ({}): {e}", cfg.codex_bin))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("write codex stdin failed: {e}"))?;
    }

    Ok(child)
}

pub(crate) fn read_output(out_path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(out_path).map_err(|e| format!("read codex output failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse codex json failed: {e}"))
}
