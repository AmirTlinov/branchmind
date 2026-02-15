#![forbid(unsafe_code)]

use crate::RunnerConfig;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub(crate) struct SpawnExecRequest<'a> {
    pub(crate) schema_path: &'a Path,
    pub(crate) out_path: &'a Path,
    pub(crate) stderr_path: &'a Path,
    pub(crate) prompt: &'a str,
    pub(crate) executor_profile: &'a str,
    pub(crate) model: Option<&'a str>,
    pub(crate) disable_mcp: bool,
}

fn append_exec_args(cmd: &mut Command, req: &SpawnExecRequest<'_>) {
    cmd.arg("exec")
        .arg("--skip-git-repo-check")
        .arg("-c")
        .arg("approval_policy=\"never\"")
        .arg("-s")
        .arg("workspace-write")
        .arg("--output-schema")
        .arg(req.schema_path)
        .arg("--output-last-message")
        .arg(req.out_path);
    if req.disable_mcp {
        // Pipeline strict mode MUST NOT mutate BranchMind state via tool calls.
        // We hard-disable MCP servers for the Codex session (best-effort guardrail).
        cmd.arg("--ephemeral").arg("-c").arg("mcp_servers={}");
    }
    if let Some(model) = req.model {
        cmd.arg("--model").arg(model);
    }
    if req.executor_profile.eq_ignore_ascii_case("xhigh") {
        // Best-effort codex effort hint: high reasoning effort for flagship quality slices.
        cmd.arg("-c").arg("model_reasoning_effort=\"high\"");
    }
}

fn append_exec_env(cmd: &mut Command, req: &SpawnExecRequest<'_>) -> Result<(), String> {
    if !req.disable_mcp {
        return Ok(());
    }

    // Force Codex to a clean home dir for strict pipeline roles, so global
    // ~/.codex profiles/servers cannot bleed into scout/builder/validator flows.
    let base_dir = req
        .out_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("bm_runner_codex"));
    let codex_home = base_dir.join("codex_home");
    std::fs::create_dir_all(&codex_home)
        .map_err(|e| format!("create isolated CODEX_HOME failed: {e}"))?;
    seed_codex_auth(&codex_home)?;
    cmd.env("CODEX_HOME", codex_home);
    Ok(())
}

fn seed_codex_auth(codex_home: &Path) -> Result<(), String> {
    let dest = codex_home.join("auth.json");
    if dest.exists() {
        return Ok(());
    }

    let Some(src) = discover_auth_source() else {
        return Ok(());
    };
    std::fs::copy(&src, &dest)
        .map_err(|e| format!("copy codex auth into isolated CODEX_HOME failed ({src:?}): {e}"))?;
    Ok(())
}

fn discover_auth_source() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(code_home) = std::env::var("CODEX_HOME") {
        candidates.push(PathBuf::from(code_home).join("auth.json"));
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".codex").join("auth.json"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub(crate) fn spawn_exec(cfg: &RunnerConfig, req: SpawnExecRequest<'_>) -> Result<Child, String> {
    let stderr_file = File::create(req.stderr_path)
        .map_err(|e| format!("create codex stderr capture failed: {e}"))?;

    let mut cmd = Command::new(&cfg.codex_bin);
    append_exec_args(&mut cmd, &req);
    append_exec_env(&mut cmd, &req)?;

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("failed to spawn codex exec ({}): {e}", cfg.codex_bin))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(req.prompt.as_bytes())
            .map_err(|e| format!("write codex stdin failed: {e}"))?;
    }

    Ok(child)
}

pub(crate) fn read_output(out_path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(out_path).map_err(|e| format!("read codex output failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse codex json failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(disable_mcp: bool) -> SpawnExecRequest<'a> {
        SpawnExecRequest {
            schema_path: Path::new("schema.json"),
            out_path: Path::new("out.json"),
            stderr_path: Path::new("stderr.log"),
            prompt: "prompt",
            executor_profile: "xhigh",
            model: Some("gpt-5.3-codex"),
            disable_mcp,
        }
    }

    fn args_for(req: &SpawnExecRequest<'_>) -> Vec<String> {
        let mut cmd = Command::new("codex");
        append_exec_args(&mut cmd, req);
        cmd.get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>()
    }

    #[test]
    fn strict_builder_disables_mcp_servers() {
        let args = args_for(&request(true));
        assert!(
            args.iter().any(|arg| arg == "--ephemeral"),
            "expected --ephemeral in strict builder mode: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-c" && pair[1] == "mcp_servers={}"),
            "expected mcp_servers override in strict builder mode: {args:?}"
        );
    }

    #[test]
    fn non_strict_mode_keeps_default_mcp_behavior() {
        let args = args_for(&request(false));
        assert!(
            !args.iter().any(|arg| arg == "--ephemeral"),
            "did not expect --ephemeral outside strict builder mode: {args:?}"
        );
        assert!(
            !args
                .windows(2)
                .any(|pair| pair[0] == "-c" && pair[1] == "mcp_servers={}"),
            "did not expect mcp_servers override outside strict builder mode: {args:?}"
        );
    }
}
