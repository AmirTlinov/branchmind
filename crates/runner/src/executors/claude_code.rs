#![forbid(unsafe_code)]

use crate::RunnerConfig;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};

fn append_exec_args(cmd: &mut Command, repo_root: &Path, schema_json: &str, model: Option<&str>) {
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
        .arg(repo_root.to_string_lossy().to_string())
        // Deterministic runner mode: do not load user/global MCP/plugin servers.
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg("{\"mcpServers\":{}}");

    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }
}

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
    append_exec_args(&mut cmd, &cfg.repo_root, schema_json, model);

    // `claude` does not accept a `--cwd` flag (as of 2.x); set the process working directory
    // directly to keep file operations deterministic and within the repo.
    cmd.current_dir(&cfg.repo_root);

    let mut child = cmd
        // Claude Code requires the structured input via stdin when using `--print` / JSON output.
        // Passing the prompt as argv is brittle (ARG_MAX, quoting) and fails on some versions.
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("failed to spawn claude ({claude_bin}): {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("write claude stdin failed: {e}"))?;
    }

    Ok(child)
}

pub(crate) fn read_output(out_path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(out_path).map_err(|e| format!("read claude output failed: {e}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("parse claude json failed: {e}"))?;

    // Claude Code may return a wrapper error object when it fails to satisfy the provided
    // JSON schema after internal retries (e.g. `error_max_structured_output_retries`).
    // Treat this as a hard executor error so the runner can fail the slice deterministically,
    // instead of attempting to validate/store an invalid payload.
    if value.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        let subtype = value
            .get("subtype")
            .and_then(|v| v.as_str())
            .unwrap_or("claude_error");
        let first_error = value
            .get("errors")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        return Err(format!("claude_code: {subtype}: {first_error}"));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    fn mk_tmp_dir(prefix: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let pid = std::process::id();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        dir.push(format!("{prefix}_{pid}_{ts}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn claude_code_prompt_is_sent_via_stdin() {
        let tmp = mk_tmp_dir("bm_runner_claude_stdin");
        let seen_path = tmp.join("seen_prompt.txt");
        let out_path = tmp.join("out.json");
        let stderr_path = tmp.join("err.txt");

        // A tiny shim executable that:
        // 1) reads stdin into `seen_prompt.txt`
        // 2) prints a valid Claude JSON wrapper to stdout (captured by spawn_exec)
        let shim_path = tmp.join("claude_shim.sh");
        let shim = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
cat - > "{seen}"
printf '%s\n' '{{"type":"result","structured_output":{{"ok":true}}}}'
"#,
            seen = seen_path.to_string_lossy()
        );
        fs::write(&shim_path, shim).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&shim_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&shim_path, perms).unwrap();
        }

        let cfg = crate::RunnerConfig {
            workspace: "ws_test".to_string(),
            storage_dir: tmp.clone(),
            repo_root: tmp.clone(),
            runner_id: "runner_test".to_string(),
            poll_ms: 10,
            heartbeat_ms: 10,
            max_runtime_s: 1,
            slice_s: 1,
            slice_grace_s: 1,
            stale_after_s: 1,
            max_failures: 1,
            once: true,
            dry_run: true,
            mcp_bin: "bm_mcp".to_string(),
            codex_bin: "codex".to_string(),
            claude_bin: Some(shim_path.to_string_lossy().to_string()),
            skill_profile: "deep".to_string(),
            skill_max_chars: 1000,
        };

        let prompt = "hello from stdin";
        let mut child =
            spawn_exec(&cfg, "{}", &out_path, &stderr_path, prompt, None).expect("spawn_exec");
        let _ = child.wait();

        let mut got = String::new();
        fs::File::open(&seen_path)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, prompt);
    }

    #[test]
    fn claude_code_exec_args_enforce_strict_mcp_config() {
        let mut cmd = Command::new("claude");
        append_exec_args(&mut cmd, Path::new("/tmp/repo"), "{}", Some("haiku"));
        let args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(
            args.iter().any(|a| a == "--strict-mcp-config"),
            "expected --strict-mcp-config in runner claude args: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--mcp-config" && pair[1] == "{\"mcpServers\":{}}"),
            "expected empty strict mcp config in runner claude args: {args:?}"
        );
    }
}
