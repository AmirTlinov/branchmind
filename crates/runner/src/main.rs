#![forbid(unsafe_code)]

mod bin_detect;
mod defaults;
mod executors;
mod mcp_client;
#[cfg(test)]
mod patch_apply;
#[cfg(test)]
mod patch_types;
mod pipeline_contract;
mod prompt;
mod runtime_helpers;

use mcp_client::McpClient;
use pipeline_contract::{has_done_proof_ref, validate_pipeline_summary_contract};
use runtime_helpers::*;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_JOBS_MODEL: &str = "gpt-5.3-codex";
const SCOUT_SLICE_SLA_S: u64 = 300;
const BUILDER_SLICE_SLA_S: u64 = 1200;
const VALIDATOR_SLICE_SLA_S: u64 = 600;
const SCOUT_HEARTBEAT_SLA_MS: u64 = 15_000;
const BUILDER_HEARTBEAT_SLA_MS: u64 = 45_000;
const VALIDATOR_HEARTBEAT_SLA_MS: u64 = 30_000;
const MIN_SLICE_S: u64 = 30;
const MIN_HEARTBEAT_MS: u64 = 10_000;
const MAX_HEARTBEAT_MS: u64 = 300_000;

#[derive(Debug, Clone, Copy)]
struct SliceTiming {
    slice_s: u64,
    slice_grace_s: u64,
    heartbeat_ms: u64,
}

#[cfg(test)]
use pipeline_contract::has_non_job_ref;

#[derive(Debug)]
struct RunnerConfig {
    workspace: String,
    storage_dir: PathBuf,
    repo_root: PathBuf,
    runner_id: String,
    poll_ms: u64,
    heartbeat_ms: u64,
    max_runtime_s: u64,
    slice_s: u64,
    slice_grace_s: u64,
    stale_after_s: u64,
    max_failures: u32,
    once: bool,
    dry_run: bool,
    mcp_bin: String,
    codex_bin: String,
    claude_bin: Option<String>,
    skill_profile: String,
    skill_max_chars: usize,
}

fn usage() -> &'static str {
    "bm_runner — execute BranchMind JOB-* via headless Codex / Claude Code\n\n\
USAGE:\n\
  bm_runner [--storage-dir DIR] [--workspace WS] [--runner-id ID]\n\
            [--poll-ms MS] [--heartbeat-ms MS]\n\
            [--max-runtime-s S] [--slice-s S] [--slice-grace-s S]\n\
            [--stale-after-s S] [--once] [--dry-run]\n\
            [--mcp-bin PATH] [--codex-bin PATH] [--claude-bin PATH]\n\
            [--skill-profile PROFILE] [--skill-max-chars N]\n\n\
NOTES:\n\
  - bm_mcp stays deterministic; this runner executes jobs out-of-process.\n\
  - If `claude` is on PATH, `claude_code` is auto-detected (no flags needed).\n\
    Use `--claude-bin` / `BM_CLAUDE_BIN` to override.\n\
  - `--dry-run` claims a job and completes it immediately (smoke test).\n\
  - long jobs: the runner sends heartbeats and can time-slice Codex runs.\n"
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn runner_lease_ttl_ms(cfg: &RunnerConfig) -> u64 {
    // Keep liveness unambiguous without tight coupling to poll frequency.
    // The store also clamps TTL defensively.
    cfg.heartbeat_ms.saturating_mul(3).clamp(1_000, 300_000)
}

fn job_claim_lease_ttl_ms(cfg: &RunnerConfig) -> u64 {
    // Use the same TTL policy as the runner lease.
    // The job lease is renewed via `jobs (cmd=jobs.report)` heartbeats.
    runner_lease_ttl_ms(cfg)
}

fn normalize_skill_profile(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    match lowered.as_str() {
        "daily" | "strict" | "research" | "teamlead" => Some(lowered),
        _ => None,
    }
}

fn infer_skill_profile_from_job_kind(kind: &str) -> Option<String> {
    let k = kind.trim();
    if k.is_empty() {
        return None;
    }
    let lowered = k.to_ascii_lowercase();
    if lowered.contains("research") {
        return Some("research".to_string());
    }
    None
}

fn select_skill_profile(
    job_kind: Option<&str>,
    job_meta: Option<&Value>,
    cfg: &RunnerConfig,
) -> String {
    // Selection order (deterministic):
    // 1) job.meta.skill_profile (if valid)
    // 2) infer from job kind
    // 3) runner default
    if let Some(profile) = job_meta
        .and_then(|v| v.as_object())
        .and_then(|meta| meta.get("skill_profile"))
        .and_then(Value::as_str)
        .and_then(normalize_skill_profile)
    {
        return profile;
    }
    if let Some(profile) = job_kind
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(infer_skill_profile_from_job_kind)
    {
        return profile;
    }
    cfg.skill_profile.clone()
}

fn select_skill_max_chars(job_meta: Option<&Value>, cfg: &RunnerConfig) -> usize {
    // meta override is optional. if present and numeric:
    // - 0 disables injection for this job.
    // - otherwise clamp to a safe upper bound.
    let from_meta = job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("skill_max_chars"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    from_meta.unwrap_or(cfg.skill_max_chars).min(8000)
}

fn job_meta_selected_executor(job_meta: Option<&Value>) -> Option<&str> {
    let meta = job_meta?.as_object()?;
    if let Some(selected) = meta
        .get("routing")
        .and_then(|v| v.get("selected_executor"))
        .and_then(value_as_str)
    {
        return Some(selected);
    }
    meta.get("executor").and_then(value_as_str)
}

fn job_meta_executor_profile(job_meta: Option<&Value>) -> Option<&str> {
    job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("executor_profile"))
        .and_then(value_as_str)
}

fn job_meta_executor_model(job_meta: Option<&Value>) -> Option<&str> {
    job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("executor_model"))
        .and_then(value_as_str)
}

fn job_meta_pipeline_role(job_meta: Option<&Value>) -> Option<&str> {
    let role = job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("pipeline_role").or_else(|| m.get("role")))
        .and_then(value_as_str)?;
    let role_norm = role.trim();
    if role_norm.eq_ignore_ascii_case("scout")
        || role_norm.eq_ignore_ascii_case("builder")
        || role_norm.eq_ignore_ascii_case("validator")
        || role_norm.eq_ignore_ascii_case("writer")
    {
        Some(role_norm)
    } else {
        None
    }
}

fn job_meta_input_mode(job_meta: Option<&Value>) -> Option<&str> {
    let mode = job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("input_mode"))
        .and_then(value_as_str)?;
    if mode.eq_ignore_ascii_case("strict") {
        Some("strict")
    } else if mode.eq_ignore_ascii_case("flex") {
        Some("flex")
    } else {
        None
    }
}

fn job_meta_u64(job_meta: Option<&Value>, key: &str) -> Option<u64> {
    let value = job_meta
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(key))?;
    if let Some(v) = value.as_u64() {
        return Some(v);
    }
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
}

fn role_slice_cap_s(role: Option<&str>) -> Option<u64> {
    let role = role?;
    if role.eq_ignore_ascii_case("scout") {
        Some(SCOUT_SLICE_SLA_S)
    } else if role.eq_ignore_ascii_case("builder") || role.eq_ignore_ascii_case("writer") {
        Some(BUILDER_SLICE_SLA_S)
    } else if role.eq_ignore_ascii_case("validator") {
        Some(VALIDATOR_SLICE_SLA_S)
    } else {
        None
    }
}

fn role_heartbeat_cap_ms(role: Option<&str>) -> Option<u64> {
    let role = role?;
    if role.eq_ignore_ascii_case("scout") {
        Some(SCOUT_HEARTBEAT_SLA_MS)
    } else if role.eq_ignore_ascii_case("builder") || role.eq_ignore_ascii_case("writer") {
        Some(BUILDER_HEARTBEAT_SLA_MS)
    } else if role.eq_ignore_ascii_case("validator") {
        Some(VALIDATOR_HEARTBEAT_SLA_MS)
    } else {
        None
    }
}

fn resolve_slice_timing(
    cfg: &RunnerConfig,
    job_meta: Option<&Value>,
    pipeline_role: Option<&str>,
) -> SliceTiming {
    let base_slice = role_slice_cap_s(pipeline_role)
        .map(|cap| cfg.slice_s.min(cap))
        .unwrap_or(cfg.slice_s);
    let raw_slice = job_meta_u64(job_meta, "slice_s").unwrap_or(base_slice);
    let slice_s = raw_slice.max(MIN_SLICE_S);

    let base_grace = cfg.slice_grace_s.min(slice_s);
    let raw_grace = job_meta_u64(job_meta, "slice_grace_s").unwrap_or(base_grace);
    let slice_grace_s = raw_grace.min(slice_s);

    let role_heartbeat = role_heartbeat_cap_ms(pipeline_role).unwrap_or(cfg.heartbeat_ms);
    let base_heartbeat = cfg
        .heartbeat_ms
        .min(role_heartbeat)
        .min(slice_s.saturating_mul(1000).max(MIN_HEARTBEAT_MS));
    let raw_heartbeat = job_meta_u64(job_meta, "heartbeat_ms").unwrap_or(base_heartbeat);
    let heartbeat_ms = raw_heartbeat.clamp(MIN_HEARTBEAT_MS, MAX_HEARTBEAT_MS);

    SliceTiming {
        slice_s,
        slice_grace_s,
        heartbeat_ms,
    }
}

fn normalize_executor_profile(raw: Option<&str>) -> &'static str {
    let Some(v) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "xhigh";
    };
    if v.eq_ignore_ascii_case("fast") {
        "fast"
    } else if v.eq_ignore_ascii_case("deep") {
        "deep"
    } else if v.eq_ignore_ascii_case("audit") {
        "audit"
    } else {
        "xhigh"
    }
}

fn resolve_job_executor_plan(
    job_meta: Option<&Value>,
    cfg: &RunnerConfig,
) -> Result<(executors::ExecutorKind, &'static str, Option<String>), String> {
    let profile = normalize_executor_profile(job_meta_executor_profile(job_meta));

    let executor = job_meta_selected_executor(job_meta).unwrap_or("auto");
    let codex_available = crate::bin_detect::can_resolve_command(&cfg.codex_bin);
    let claude_available = cfg
        .claude_bin
        .as_deref()
        .is_some_and(crate::bin_detect::can_resolve_command);

    let kind = if executor.eq_ignore_ascii_case("codex") {
        executors::ExecutorKind::Codex
    } else if executor.eq_ignore_ascii_case("claude_code") {
        if !claude_available {
            return Err(
                "claude_code executor requested but the runner cannot resolve the Claude CLI (install `claude` or set --claude-bin / BM_CLAUDE_BIN)"
                    .to_string(),
            );
        }
        executors::ExecutorKind::ClaudeCode
    } else {
        // executor=auto (or unknown): deterministic local selection.
        //
        // The store may already contain a deterministic `routing.selected_executor` chosen by the
        // server. If it doesn't (e.g. job created before any runner lease existed), we fall back
        // to a simple, stable policy:
        // - prefer Claude Code for deep/audit when available,
        // - otherwise prefer Codex,
        // - if only one executor is available, use it.
        if !codex_available && claude_available {
            executors::ExecutorKind::ClaudeCode
        } else if codex_available && !claude_available {
            executors::ExecutorKind::Codex
        } else if codex_available && claude_available {
            if matches!(profile, "deep" | "audit") {
                executors::ExecutorKind::ClaudeCode
            } else {
                executors::ExecutorKind::Codex
            }
        } else {
            return Err(
                "no executors available (install `codex` and/or `claude`, or configure --codex-bin/--claude-bin)"
                    .to_string(),
            );
        }
    };

    let model = job_meta_executor_model(job_meta)
        .map(|v| v.to_string())
        .or_else(|| {
            if matches!(kind, executors::ExecutorKind::Codex) {
                Some(DEFAULT_JOBS_MODEL.to_string())
            } else {
                None
            }
        });

    Ok((kind, profile, model))
}

fn send_runner_heartbeat(
    mcp: &mut McpClient,
    cfg: &RunnerConfig,
    status: &str,
    active_job_id: Option<&str>,
) {
    let mut args = serde_json::Map::new();
    args.insert("runner_id".to_string(), json!(cfg.runner_id));
    args.insert("status".to_string(), json!(status));
    args.insert("lease_ttl_ms".to_string(), json!(runner_lease_ttl_ms(cfg)));
    if let Some(job) = active_job_id {
        args.insert("active_job_id".to_string(), json!(job));
    }
    let mut execs = Vec::<&str>::new();
    if crate::bin_detect::can_resolve_command(&cfg.codex_bin) {
        execs.push("codex");
    }
    if cfg
        .claude_bin
        .as_deref()
        .is_some_and(crate::bin_detect::can_resolve_command)
    {
        execs.push("claude_code");
    }
    args.insert("executors".to_string(), json!(execs));
    args.insert(
        "profiles".to_string(),
        json!(["fast", "deep", "audit", "xhigh"]),
    );
    args.insert(
        "supports_artifacts".to_string(),
        json!(["report", "diff", "patch", "bench", "docs_update"]),
    );
    args.insert("max_parallel".to_string(), json!(1));
    args.insert("sandbox_policy".to_string(), json!("local"));
    args.insert(
        "meta".to_string(),
        json!({
            "pid": std::process::id(),
            "poll_ms": cfg.poll_ms,
            "heartbeat_ms": cfg.heartbeat_ms
        }),
    );
    let _ = mcp.call_tool(
        "jobs",
        json!({
            "workspace": cfg.workspace,
            "op": "call",
            "cmd": "jobs.runner.heartbeat",
            "args": Value::Object(args)
        }),
    );
}

fn default_storage_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    defaults::default_storage_dir_from_start(&cwd)
}

fn default_workspace() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    defaults::default_workspace_from_start(&cwd)
}

fn default_repo_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            return cwd;
        }
    }
}

fn default_mcp_bin() -> String {
    // Prefer a sibling `bm_mcp` next to this runner binary.
    // This makes `./target/debug/bm_runner` work without requiring PATH or `--mcp-bin`.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("bm_mcp");
        if sibling.exists() {
            return sibling.to_string_lossy().to_string();
        }
    }
    "bm_mcp".to_string()
}

fn parse_args() -> Result<RunnerConfig, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", usage());
        std::process::exit(0);
    }

    let mut storage_dir: Option<PathBuf> = env_var("BM_STORAGE_DIR").map(PathBuf::from);
    let mut workspace: Option<String> = env_var("BM_WORKSPACE");
    let mut runner_id: Option<String> = env_var("BM_RUNNER_ID");
    let mut poll_ms: u64 = env_var("BM_POLL_MS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    let mut heartbeat_ms: u64 = env_var("BM_HEARTBEAT_MS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(60_000);
    let mut max_runtime_s: u64 = env_var("BM_MAX_RUNTIME_S")
        .and_then(|v| v.parse().ok())
        .unwrap_or(86_400);
    let mut slice_s: u64 = env_var("BM_SLICE_S")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_800);
    let mut slice_grace_s: u64 = env_var("BM_SLICE_GRACE_S")
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let mut stale_after_s: u64 = env_var("BM_STALE_AFTER_S")
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let mut max_failures: u32 = env_var("BM_MAX_FAILURES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let mut once = false;
    let mut dry_run = false;
    let mut mcp_bin: Option<String> = env_var("BM_MCP_BIN");
    let mut codex_bin: Option<String> = env_var("BM_CODEX_BIN");
    let mut claude_bin: Option<String> = env_var("BM_CLAUDE_BIN");
    let mut skill_profile: Option<String> = env_var("BM_SKILL_PROFILE");
    let mut skill_max_chars: usize = env_var("BM_SKILL_MAX_CHARS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1200);

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--storage-dir" => {
                i += 1;
                let v = args.get(i).ok_or("--storage-dir requires DIR")?;
                storage_dir = Some(PathBuf::from(v));
            }
            "--workspace" => {
                i += 1;
                let v = args.get(i).ok_or("--workspace requires WS")?;
                workspace = Some(v.to_string());
            }
            "--runner-id" => {
                i += 1;
                let v = args.get(i).ok_or("--runner-id requires ID")?;
                runner_id = Some(v.to_string());
            }
            "--poll-ms" => {
                i += 1;
                let v = args.get(i).ok_or("--poll-ms requires MS")?;
                poll_ms = v
                    .parse::<u64>()
                    .map_err(|_| "--poll-ms must be an integer (milliseconds)")?;
            }
            "--heartbeat-ms" => {
                i += 1;
                let v = args.get(i).ok_or("--heartbeat-ms requires MS")?;
                heartbeat_ms = v
                    .parse::<u64>()
                    .map_err(|_| "--heartbeat-ms must be an integer (milliseconds)")?;
            }
            "--max-runtime-s" => {
                i += 1;
                let v = args.get(i).ok_or("--max-runtime-s requires S")?;
                max_runtime_s = v
                    .parse::<u64>()
                    .map_err(|_| "--max-runtime-s must be an integer (seconds)")?;
            }
            "--slice-s" => {
                i += 1;
                let v = args.get(i).ok_or("--slice-s requires S")?;
                slice_s = v
                    .parse::<u64>()
                    .map_err(|_| "--slice-s must be an integer (seconds)")?;
            }
            "--slice-grace-s" => {
                i += 1;
                let v = args.get(i).ok_or("--slice-grace-s requires S")?;
                slice_grace_s = v
                    .parse::<u64>()
                    .map_err(|_| "--slice-grace-s must be an integer (seconds)")?;
            }
            "--stale-after-s" => {
                i += 1;
                let v = args.get(i).ok_or("--stale-after-s requires S")?;
                stale_after_s = v
                    .parse::<u64>()
                    .map_err(|_| "--stale-after-s must be an integer (seconds)")?;
            }
            "--max-failures" => {
                i += 1;
                let v = args.get(i).ok_or("--max-failures requires N")?;
                max_failures = v
                    .parse::<u32>()
                    .map_err(|_| "--max-failures must be an integer")?;
            }
            "--once" => once = true,
            "--dry-run" => dry_run = true,
            "--mcp-bin" => {
                i += 1;
                let v = args.get(i).ok_or("--mcp-bin requires PATH")?;
                mcp_bin = Some(v.to_string());
            }
            "--codex-bin" => {
                i += 1;
                let v = args.get(i).ok_or("--codex-bin requires PATH")?;
                codex_bin = Some(v.to_string());
            }
            "--claude-bin" => {
                i += 1;
                let v = args.get(i).ok_or("--claude-bin requires PATH")?;
                claude_bin = Some(v.to_string());
            }
            "--skill-profile" => {
                i += 1;
                let v = args.get(i).ok_or("--skill-profile requires PROFILE")?;
                skill_profile = Some(v.to_string());
            }
            "--skill-max-chars" => {
                i += 1;
                let v = args.get(i).ok_or("--skill-max-chars requires N")?;
                skill_max_chars = v
                    .parse::<usize>()
                    .map_err(|_| "--skill-max-chars must be an integer")?;
            }
            other => return Err(format!("Unknown arg: {other}\n\n{}", usage())),
        }
        i += 1;
    }

    let storage_dir = storage_dir.unwrap_or_else(default_storage_dir);
    let workspace = workspace.unwrap_or_else(default_workspace);
    let runner_id = runner_id.unwrap_or_else(|| format!("bm_runner:{}", std::process::id()));
    let mcp_bin = mcp_bin.unwrap_or_else(default_mcp_bin);
    let codex_bin = codex_bin.unwrap_or_else(|| "codex".to_string());
    let claude_bin = crate::bin_detect::resolve_optional_bin(claude_bin, "claude");
    let skill_profile = skill_profile.unwrap_or_else(|| "strict".to_string());
    let skill_profile = normalize_skill_profile(&skill_profile)
        .ok_or("invalid --skill-profile (expected daily|strict|research|teamlead)")?;

    Ok(RunnerConfig {
        workspace,
        storage_dir,
        repo_root: default_repo_root(),
        runner_id,
        poll_ms,
        heartbeat_ms,
        max_runtime_s,
        slice_s,
        slice_grace_s,
        stale_after_s,
        max_failures,
        once,
        dry_run,
        mcp_bin,
        codex_bin,
        claude_bin,
        skill_profile,
        skill_max_chars,
    })
}

use prompt::{build_subagent_prompt, render_job_thread, sanitize_single_line};

fn first_job_from_list(list: &Value) -> Option<Value> {
    list.get("jobs")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
}

fn first_stale_running_job(list: &Value, _stale_after_s: u64) -> Option<Value> {
    let now = now_ms();
    list.get("jobs").and_then(|v| v.as_array()).and_then(|arr| {
        arr.iter()
            .filter(|row| {
                row.get("status")
                    .and_then(value_as_str)
                    .map(|s| s.eq_ignore_ascii_case("RUNNING"))
                    .unwrap_or(false)
            })
            .filter(
                |row| match row.get("claim_expires_at_ms").and_then(value_as_i64) {
                    Some(expires_at_ms) => expires_at_ms <= now,
                    None => true,
                },
            )
            // jobs_list is ordered newest-first; stale jobs tend to be at the end. pick the most stale within the bounded list.
            .min_by_key(|row| row.get("updated_at_ms").and_then(value_as_i64).unwrap_or(0))
            .cloned()
    })
}

fn format_exit_status(status: &std::process::ExitStatus) -> String {
    status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".to_string())
}

fn read_stderr_snippet(stderr_path: &Path, head_chars: usize, tail_chars: usize) -> String {
    let raw = match std::fs::read(stderr_path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => return "-".to_string(),
    };
    let sanitized = sanitize_single_line(&raw).trim().to_string();
    if sanitized.is_empty() {
        return "-".to_string();
    }
    let total = sanitized.chars().count();
    if head_chars == 0 && tail_chars == 0 {
        return "-".to_string();
    }
    if head_chars == 0 {
        return sanitized
            .chars()
            .skip(total.saturating_sub(tail_chars))
            .collect::<String>();
    }
    if tail_chars == 0 {
        return sanitized.chars().take(head_chars).collect::<String>();
    }
    if total <= head_chars.saturating_add(tail_chars).saturating_add(3) {
        return sanitized;
    }
    let head = sanitized.chars().take(head_chars).collect::<String>();
    let tail = sanitized
        .chars()
        .skip(total.saturating_sub(tail_chars))
        .collect::<String>();
    format!("{head} … {tail}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_args().map_err(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    })?;

    let mut mcp = McpClient::spawn(&cfg.mcp_bin, &cfg.storage_dir, &cfg.workspace)
        .map_err(|e| {
            eprintln!("{e}");
            std::process::exit(2);
        })
        .unwrap();
    mcp.initialize().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });

    // Explicit runner liveness: publish an initial idle lease right away.
    let mut last_runner_beat_ms = now_ms();
    send_runner_heartbeat(&mut mcp, &cfg, "idle", None);

    let tmp_dir = std::env::temp_dir().join(format!("bm_runner_{}", std::process::id()));
    let schema_path_default =
        executors::output_schema::write_job_output_schema_file_for_role(&tmp_dir, None)
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_path_scout =
        executors::output_schema::write_job_output_schema_file_for_role(&tmp_dir, Some("scout"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_path_builder =
        executors::output_schema::write_job_output_schema_file_for_role(&tmp_dir, Some("builder"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_path_validator = executors::output_schema::write_job_output_schema_file_for_role(
        &tmp_dir,
        Some("validator"),
    )
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let schema_path_writer =
        executors::output_schema::write_job_output_schema_file_for_role(&tmp_dir, Some("writer"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });

    let schema_json_default = executors::output_schema::job_output_schema_json_arg_for_role(None)
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2);
        });
    let schema_json_scout =
        executors::output_schema::job_output_schema_json_arg_for_role(Some("scout"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_json_builder =
        executors::output_schema::job_output_schema_json_arg_for_role(Some("builder"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_json_validator =
        executors::output_schema::job_output_schema_json_arg_for_role(Some("validator"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });
    let schema_json_writer =
        executors::output_schema::job_output_schema_json_arg_for_role(Some("writer"))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            });

    loop {
        // Prefer queued jobs. If none, attempt to reclaim a stale RUNNING job.
        let queued = mcp.call_tool(
            "jobs",
            json!({
                "workspace": cfg.workspace,
                "op": "call",
                "cmd": "jobs.list",
                "args": {
                    "status": "QUEUED",
                    "limit": 1,
                    "max_chars": 4000
                }
            }),
        )?;
        let candidate = if let Some(job) = first_job_from_list(&queued) {
            Some(job)
        } else {
            // Bounded scan for stale RUNNING jobs (ralf-loop recovery).
            let running = mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.list",
                    "args": {
                        "status": "RUNNING",
                        "limit": 200,
                        "max_chars": 12000
                    }
                }),
            )?;
            first_stale_running_job(&running, cfg.stale_after_s)
        };

        let Some(job) = candidate else {
            if cfg.once {
                break;
            }
            let now = now_ms();
            if now.saturating_sub(last_runner_beat_ms) >= cfg.heartbeat_ms as i64 {
                last_runner_beat_ms = now;
                send_runner_heartbeat(&mut mcp, &cfg, "idle", None);
            }
            sleep(Duration::from_millis(cfg.poll_ms));
            continue;
        };

        let Some(job_id) = job.get("job_id").and_then(value_as_str) else {
            // Malformed row; avoid busy-looping.
            sleep(Duration::from_millis(cfg.poll_ms));
            continue;
        };

        let is_reclaim = job
            .get("status")
            .and_then(value_as_str)
            .map(|s| s.eq_ignore_ascii_case("RUNNING"))
            .unwrap_or(false);

        // Attempt to claim. If not claimable, another runner got it; continue polling.
        let claim_args = if is_reclaim {
            json!({
                "job": job_id,
                "runner_id": cfg.runner_id,
                "allow_stale": true,
                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg)
            })
        } else {
            json!({
                "job": job_id,
                "runner_id": cfg.runner_id,
                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg)
            })
        };
        let claim = mcp.call_tool(
            "jobs",
            json!({
                "workspace": cfg.workspace,
                "op": "call",
                "cmd": "jobs.claim",
                "args": claim_args
            }),
        );
        let claim = match claim {
            Ok(v) => v,
            Err(_) => {
                if cfg.once {
                    break;
                }
                let now = now_ms();
                if now.saturating_sub(last_runner_beat_ms) >= cfg.heartbeat_ms as i64 {
                    last_runner_beat_ms = now;
                    send_runner_heartbeat(&mut mcp, &cfg, "idle", None);
                }
                sleep(Duration::from_millis(cfg.poll_ms));
                continue;
            }
        };

        let claim_revision = claim
            .get("job")
            .and_then(|v| v.get("revision"))
            .and_then(value_as_i64)
            .unwrap_or(-1);
        if claim_revision < 0 {
            // Malformed claim response; avoid looping in a bad state.
            last_runner_beat_ms = now_ms();
            send_runner_heartbeat(&mut mcp, &cfg, "idle", None);
            if cfg.once {
                break;
            }
            sleep(Duration::from_millis(cfg.poll_ms));
            continue;
        }

        // We own the job lease now; mark runner as live immediately (no ambiguity in inbox).
        send_runner_heartbeat(&mut mcp, &cfg, "live", Some(job_id));

        let open_args = json!({
            "job": job_id,
            "include_prompt": true,
            "include_events": false,
            "max_events": 0,
            "include_meta": true,
            "max_chars": 8000
        });
        let open = mcp
            .call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.open",
                    "args": open_args
                }),
            )
            .or_else(|_| {
                mcp.call_tool(
                    "jobs",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "jobs.open",
                        "args": {
                            "job": job_id,
                            "include_prompt": true,
                            "include_events": false,
                            "max_events": 0,
                            "max_chars": 8000
                        }
                    }),
                )
            })?;

        let prompt = open
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let job_row = open.get("job").cloned().unwrap_or(Value::Null);
        let job_meta = open.get("meta");
        let job_priority = job_row
            .get("priority")
            .and_then(value_as_str)
            .unwrap_or("MEDIUM")
            .to_string();
        let base_meta = job_meta
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let pipeline_role = job_meta_pipeline_role(job_meta);
        let builder_input_only = pipeline_role
            .is_some_and(|role| role.eq_ignore_ascii_case("builder"))
            && job_meta_input_mode(job_meta)
                .is_some_and(|mode| mode.eq_ignore_ascii_case("strict"));
        let slice_timing = resolve_slice_timing(&cfg, job_meta, pipeline_role);

        let (executor_kind, executor_profile, executor_model) =
            match resolve_job_executor_plan(job_meta, &cfg) {
                Ok(v) => v,
                Err(err) => {
                    let _ = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.complete",
                            "args": {
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "status": "FAILED",
                                "summary": format!("runner: unsupported executor: {err}"),
                                "refs": [ job_id ],
                                "meta": { "runner": cfg.runner_id }
                            }
                        }),
                    );

                    last_runner_beat_ms = now_ms();
                    send_runner_heartbeat(&mut mcp, &cfg, "idle", None);
                    if cfg.once {
                        break;
                    }
                    continue;
                }
            };

        let task_id = job_row
            .get("task")
            .and_then(value_as_str)
            .map(|s| s.to_string());
        let anchor_id = job_row
            .get("anchor")
            .and_then(value_as_str)
            .map(|s| s.to_string());
        let job_kind = job_row.get("kind").and_then(value_as_str);

        let selected_skill_profile = select_skill_profile(job_kind, job_meta, &cfg);
        let selected_skill_max_chars = select_skill_max_chars(job_meta, &cfg);
        let skill_pack = if selected_skill_max_chars == 0 {
            String::new()
        } else {
            match mcp.call_tool(
                "system",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "system.skill",
                    "args": {
                        "profile": selected_skill_profile,
                        "max_chars": selected_skill_max_chars
                    }
                }),
            ) {
                Ok(Value::String(s)) => s,
                Ok(other) => other.to_string(),
                Err(_) => String::new(),
            }
        };

        let anchor_snapshot = if let Some(anchor_id) = anchor_id.as_deref() {
            match mcp.call_tool(
                "think",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "think.anchor.snapshot",
                    "args": {
                        "anchor": anchor_id,
                        "include_drafts": false,
                        "limit": 20,
                        "max_chars": 2500
                    }
                }),
            )? {
                Value::String(s) => s,
                other => other.to_string(),
            }
        } else {
            String::new()
        };

        if cfg.dry_run {
            let _ = mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.report",
                    "args": {
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                        "kind": "checkpoint",
                        "message": "dry-run: claimed and completing immediately",
                        "percent": 0,
                        "refs": [ job_id ],
                        "meta": { "dry_run": true, "step": { "command": "dry_run", "result": "dry-run: claimed and completing immediately" } }
                    }
                }),
            );
            let _ = mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.complete",
                    "args": {
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "status": "DONE",
                        "summary": "dry-run complete",
                        "refs": [ job_id ],
                        "meta": { "dry_run": true }
                    }
                }),
            );
            last_runner_beat_ms = now_ms();
            send_runner_heartbeat(&mut mcp, &cfg, "idle", None);
            if cfg.once {
                break;
            }
            continue;
        }

        let job_started_ms = now_ms();
        let max_runtime_ms = (cfg.max_runtime_s as i64).saturating_mul(1000);

        let mut slice_index: u64 = 0;
        let mut failures: u32 = 0;
        let mut last_summary: Option<String> = None;
        let mut last_refs: Vec<String> = Vec::new();

        'job_loop: loop {
            let now = now_ms();
            if now.saturating_sub(job_started_ms) >= max_runtime_ms {
                let _ = mcp.call_tool(
                    "jobs",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "jobs.complete",
                        "args": {
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "status": "FAILED",
                            "summary": "runner: max runtime exceeded",
                            "refs": [ job_id ],
                            "meta": {
                                "runner": cfg.runner_id,
                                "slice_index": slice_index,
                                "max_runtime_s": cfg.max_runtime_s
                            }
                        }
                    }),
                );
                break 'job_loop;
            }

            let snapshot_lines = if let Some(task_id) = task_id.as_deref() {
                match mcp.call_tool(
                    "tasks",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "tasks.snapshot",
                        "args": {
                            "task": task_id,
                            "fmt": "lines",
                            "refs": true,
                            "max_chars": 7000
                        }
                    }),
                ) {
                    Ok(Value::String(s)) => normalize_task_snapshot_lines(&s),
                    Ok(other) => normalize_task_snapshot_lines(&other.to_string()),
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            };

            let job_thread = match mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.open",
                    "args": {
                        "job": job_id,
                        "include_prompt": false,
                        "include_events": true,
                        "max_events": 40,
                        "max_chars": 6000
                    }
                }),
            ) {
                Ok(opened) => render_job_thread(&opened),
                Err(_) => String::new(),
            };

            let slice_context = if slice_index == 0 {
                format!(
                    "TASK SNAPSHOT (bounded):\n{snapshot_lines}\n\nANCHOR SNAPSHOT (bounded):\n{anchor_snapshot}\n\nJOB THREAD (recent, bounded):\n{job_thread}\n"
                )
            } else {
                let prior = last_summary.as_deref().unwrap_or("-");
                let refs_line = if last_refs.is_empty() {
                    String::new()
                } else {
                    last_refs.join(", ")
                };
                format!(
                    "PRIOR SLICE SUMMARY:\n{prior}\n\nPRIOR REFS:\n{refs_line}\n\nTASK SNAPSHOT (bounded):\n{snapshot_lines}\n\nANCHOR SNAPSHOT (bounded):\n{anchor_snapshot}\n\nJOB THREAD (recent, bounded):\n{job_thread}\n"
                )
            };

            let full_prompt = build_subagent_prompt(
                &cfg,
                job_id,
                &prompt,
                &slice_context,
                &skill_pack,
                pipeline_role,
                job_meta,
            );

            let _ = mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.report",
                    "args": {
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                        "kind": "checkpoint",
                        "message": format!("runner: slice {} started{}", slice_index, if is_reclaim { " (reclaim)" } else { "" }),
                        "percent": 0,
                        "refs": [ job_id ],
                        "meta": merged_meta(&base_meta, json!({
                            "runner": cfg.runner_id,
                            "slice_index": slice_index,
                            "reclaim": is_reclaim,
                            "slice_s": slice_timing.slice_s,
                            "slice_grace_s": slice_timing.slice_grace_s,
                            "heartbeat_ms": slice_timing.heartbeat_ms,
                            "step": {
                                "command": "slice.start",
                                "result": format!("runner: slice {} started{}", slice_index, if is_reclaim { " (reclaim)" } else { "" })
                            }
                        }))
                    }
                }),
            );

            let out_path = tmp_dir.join(format!("job_{job_id}_slice_{slice_index}.json"));
            let stderr_path = tmp_dir.join(format!("job_{job_id}_slice_{slice_index}.stderr"));
            let executor = executor_kind.as_str();
            let (schema_path_for_exec, schema_json_for_exec) = match pipeline_role {
                Some(role) if role.eq_ignore_ascii_case("scout") => {
                    (&schema_path_scout, &schema_json_scout)
                }
                Some(role) if role.eq_ignore_ascii_case("builder") => {
                    (&schema_path_builder, &schema_json_builder)
                }
                Some(role) if role.eq_ignore_ascii_case("validator") => {
                    (&schema_path_validator, &schema_json_validator)
                }
                Some(role) if role.eq_ignore_ascii_case("writer") => {
                    (&schema_path_writer, &schema_json_writer)
                }
                _ => (&schema_path_default, &schema_json_default),
            };
            let child_res = match executor_kind {
                executors::ExecutorKind::Codex => executors::codex::spawn_exec(
                    &cfg,
                    schema_path_for_exec,
                    &out_path,
                    &stderr_path,
                    &full_prompt,
                    executor_profile,
                    executor_model.as_deref(),
                ),
                executors::ExecutorKind::ClaudeCode => executors::claude_code::spawn_exec(
                    &cfg,
                    schema_json_for_exec,
                    &out_path,
                    &stderr_path,
                    &full_prompt,
                    executor_model.as_deref(),
                ),
            };
            let mut child = match child_res {
                Ok(c) => c,
                Err(err) => {
                    failures = failures.saturating_add(1);
                    let _ = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.report",
                            "args": {
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                                "kind": "error",
                                "message": format!(
                                    "runner: spawn failed (executor={executor}, failures={failures}): {err}"
                                ),
                                "percent": 0,
                                "refs": [ job_id ]
                            }
                        }),
                    );
                    if failures >= cfg.max_failures {
                        let _ = mcp.call_tool(
                            "jobs",
                            json!({
                                "workspace": cfg.workspace,
                                "op": "call",
                                "cmd": "jobs.complete",
                                "args": {
                                    "job": job_id,
                                    "runner_id": cfg.runner_id,
                                    "claim_revision": claim_revision,
                                    "status": "FAILED",
                                    "summary": format!(
                                        "runner: spawn failures exceeded (executor={executor})"
                                    ),
                                    "refs": [ job_id ],
                                    "meta": {
                                        "runner": cfg.runner_id,
                                        "failures": failures,
                                        "executor": executor,
                                        "executor_profile": executor_profile
                                    }
                                }
                            }),
                        );
                        break 'job_loop;
                    }
                    slice_index = slice_index.saturating_add(1);
                    continue 'job_loop;
                }
            };

            let slice_started_ms = now_ms();
            let slice_soft_ms = (slice_timing.slice_s as i64).saturating_mul(1000);
            let slice_hard_ms = slice_soft_ms
                .saturating_add((slice_timing.slice_grace_s as i64).saturating_mul(1000));
            let mut last_beat_ms = slice_started_ms;
            let mut killed = false;
            let mut aborted = false;
            let mut exit_status: Option<std::process::ExitStatus> = None;

            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        exit_status = Some(status);
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {}
                }
                if exit_status.is_some() {
                    break;
                }
                let now = now_ms();
                if now.saturating_sub(last_beat_ms) >= slice_timing.heartbeat_ms as i64 {
                    last_beat_ms = now;
                    send_runner_heartbeat(&mut mcp, &cfg, "live", Some(job_id));
                    let beat = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.report",
                            "args": {
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                                "kind": "heartbeat",
                                "message": format!("runner: heartbeat (slice {})", slice_index),
                                "percent": 0,
                                "refs": [ job_id ]
                            }
                        }),
                    );
                    if let Err(err) = beat {
                        // If the manager canceled/completed the job while we were running, stop
                        // quickly to avoid wasting a full slice.
                        let lowered = err.to_ascii_lowercase();
                        if lowered.contains("job is not running")
                            || lowered.contains("already terminal")
                            || lowered.contains("claim mismatch")
                        {
                            aborted = true;
                            killed = true;
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                    }
                }
                if now.saturating_sub(slice_started_ms) >= slice_hard_ms {
                    killed = true;
                    let _ = child.kill();
                    exit_status = child.wait().ok();
                    break;
                }
                sleep(Duration::from_millis(250));
            }

            if aborted {
                break 'job_loop;
            }

            if exit_status.is_none() {
                exit_status = child.wait().ok();
            }

            if let Some(status) = &exit_status
                && !status.success()
            {
                failures = failures.saturating_add(1);
                let stderr_snip = read_stderr_snippet(&stderr_path, 0, 400);
                let exit = format_exit_status(status);
                let _ = mcp.call_tool(
                    "jobs",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "jobs.report",
                        "args": {
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                            "kind": "error",
                            "message": format!(
                                "runner: {executor} exec failed (exit={exit}, killed={killed}, failures={failures}): {stderr_snip}"
                            ),
                            "percent": 0,
                            "refs": [ job_id ]
                        }
                    }),
                );
                if failures >= cfg.max_failures {
                    let _ = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.complete",
                            "args": {
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "status": "FAILED",
                                "summary": format!("runner: {executor} exec failures exceeded"),
                                "refs": [ job_id ],
                                "meta": { "runner": cfg.runner_id, "failures": failures }
                            }
                        }),
                    );
                    break 'job_loop;
                }
                slice_index = slice_index.saturating_add(1);
                continue 'job_loop;
            }

            let outcome = match executor_kind {
                executors::ExecutorKind::Codex => executors::codex::read_output(&out_path),
                executors::ExecutorKind::ClaudeCode => {
                    executors::claude_code::read_output(&out_path)
                }
            };
            let v = match outcome {
                Ok(v) => v,
                Err(err) => {
                    failures = failures.saturating_add(1);
                    let stderr_snip = read_stderr_snippet(&stderr_path, 0, 400);
                    let exit = exit_status
                        .as_ref()
                        .map(format_exit_status)
                        .unwrap_or_else(|| "-".to_string());
                    let _ = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.report",
                            "args": {
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                                "kind": "error",
                                "message": format!("runner: {executor} output unreadable (exit={exit}, killed={killed}, failures={failures}): {err}; stderr: {stderr_snip}"),
                                "percent": 0,
                                "refs": [ job_id ]
                            }
                        }),
                    );
                    if failures >= cfg.max_failures {
                        let _ = mcp.call_tool(
                            "jobs",
                            json!({
                                "workspace": cfg.workspace,
                                "op": "call",
                                "cmd": "jobs.complete",
                                "args": {
                                    "job": job_id,
                                    "runner_id": cfg.runner_id,
                                    "claim_revision": claim_revision,
                                    "status": "FAILED",
                                    "summary": "runner: output failures exceeded",
                                    "refs": [ job_id ],
                                    "meta": { "runner": cfg.runner_id, "failures": failures }
                                }
                            }),
                        );
                        break 'job_loop;
                    }
                    slice_index = slice_index.saturating_add(1);
                    continue 'job_loop;
                }
            };

            let status = v.get("status").and_then(value_as_str).unwrap_or("FAILED");
            let mut agent_summary = summary_value_to_text(v.get("summary"));
            if status.eq_ignore_ascii_case("DONE")
                && pipeline_role.is_some_and(|role| role.eq_ignore_ascii_case("builder"))
            {
                agent_summary = normalize_builder_summary_revision(&agent_summary, claim_revision);
            }
            let mut refs = v
                .get("refs")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(Vec::new);
            // Salvage proof-like refs that may have been placed into summary by mistake.
            for r in salvage_proof_refs_from_text(&agent_summary) {
                if !refs.iter().any(|existing| existing == &r) {
                    refs.push(r);
                }
            }
            if !refs.iter().any(|r| r == job_id) {
                refs.push(job_id.to_string());
            }
            if status.eq_ignore_ascii_case("DONE") && builder_input_only {
                let tool_calls = detect_tool_calls_from_stderr(&stderr_path, 12);
                if !tool_calls.is_empty() {
                    let slice_id_hint = base_meta.get("slice_id").and_then(value_as_str);
                    agent_summary = build_builder_input_only_context_request_summary(
                        slice_id_hint,
                        claim_revision,
                        &tool_calls,
                        &stderr_path,
                    );
                    let stderr_ref = format!("FILE:{}", stderr_path.display());
                    if !refs.iter().any(|r| r == &stderr_ref) {
                        refs.push(stderr_ref);
                    }
                    let guard_ref = "CMD: builder input-only guard triggered context_request";
                    if !refs.iter().any(|r| r == guard_ref) {
                        refs.push(guard_ref.to_string());
                    }
                }
            }

            // Optional structured feedback events (delegated agent -> manager).
            // These are low-noise breadcrumbs intended for supervision and navigation.
            let mut event_refs_union = Vec::<String>::new();
            if let Some(events) = v.get("events").and_then(|v| v.as_array()) {
                for ev in events {
                    let kind = ev
                        .get("kind")
                        .and_then(value_as_str)
                        .unwrap_or("progress")
                        .to_string();
                    let kind_norm = kind.trim().to_ascii_lowercase();
                    let message = ev
                        .get("message")
                        .and_then(value_as_str)
                        .unwrap_or("-")
                        .to_string();
                    let percent = ev.get("percent").and_then(value_as_i64);
                    let mut ev_refs = ev
                        .get("refs")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    // If the agent placed refs into the message (common mistake), salvage them.
                    ev_refs.extend(salvage_proof_refs_from_text(&message));
                    // Track union (so DONE can pass proof-gate even if proof refs were only in events).
                    for r in &ev_refs {
                        if !event_refs_union.iter().any(|existing| existing == r) {
                            event_refs_union.push(r.clone());
                        }
                    }
                    // If no explicit refs were provided for this event, fall back to the job refs
                    // (keeps job thread navigable while staying deterministic).
                    if ev_refs.is_empty() {
                        ev_refs = refs.clone();
                    }
                    if !ev_refs.iter().any(|r| r == job_id) {
                        ev_refs.push(job_id.to_string());
                    }

                    let mut report_args = serde_json::Map::new();
                    report_args.insert("job".to_string(), json!(job_id));
                    report_args.insert("runner_id".to_string(), json!(cfg.runner_id));
                    report_args.insert("claim_revision".to_string(), json!(claim_revision));
                    report_args.insert(
                        "lease_ttl_ms".to_string(),
                        json!(job_claim_lease_ttl_ms(&cfg)),
                    );
                    report_args.insert("kind".to_string(), json!(kind));
                    report_args.insert("message".to_string(), json!(message));
                    report_args.insert("refs".to_string(), json!(ev_refs));
                    if let Some(p) = percent {
                        report_args.insert("percent".to_string(), json!(p));
                    }
                    // Strict schema requires meta.step for progress/checkpoint. Keep job meta stable by
                    // merging the base job meta with a small step envelope.
                    if matches!(kind_norm.as_str(), "progress" | "checkpoint") {
                        report_args.insert(
                            "meta".to_string(),
                            merged_meta(
                                &base_meta,
                                json!({
                                    "runner": cfg.runner_id,
                                    "slice_index": slice_index,
                                    "agent_event": true,
                                    "step": {
                                        "command": format!("agent.{kind_norm}"),
                                        "result": message.clone()
                                    }
                                }),
                            ),
                        );
                    }
                    let _ = mcp.call_tool(
                        "jobs",
                        json!({
                            "workspace": cfg.workspace,
                            "op": "call",
                            "cmd": "jobs.report",
                            "args": Value::Object(report_args)
                        }),
                    );
                }
            }
            // Merge event refs into job refs (prevents false proof-gate CONTINUE).
            for r in event_refs_union {
                if !refs.iter().any(|existing| existing == &r) {
                    refs.push(r);
                }
            }

            if status.eq_ignore_ascii_case("DONE")
                && let Some(role) = pipeline_role.map(|v| v.trim().to_ascii_lowercase())
            {
                let artifact_ref = match role.as_str() {
                    "scout" => Some(format!("artifact://jobs/{job_id}/scout_context_pack")),
                    "builder" => Some(format!("artifact://jobs/{job_id}/builder_diff_batch")),
                    "validator" => Some(format!("artifact://jobs/{job_id}/validator_report")),
                    _ => None,
                };
                if let Some(artifact_ref) = artifact_ref
                    && !refs.iter().any(|r| r == &artifact_ref)
                {
                    refs.push(artifact_ref);
                }
            }

            failures = 0;
            let mut prior_summary = agent_summary.clone();
            last_refs = refs.clone();

            let mut effective_status = status;
            let mut checkpoint_summary = agent_summary.clone();
            let mut proof_gate_triggered = false;

            if status.eq_ignore_ascii_case("DONE")
                && let Some(role) = pipeline_role
                && let Err(contract_err) = validate_pipeline_summary_contract(role, &agent_summary)
            {
                effective_status = "CONTINUE";
                proof_gate_triggered = true;
                checkpoint_summary = format!("contract gate ({role}): {contract_err}");
                prior_summary = format!("{agent_summary}\n\n{checkpoint_summary}");
            }

            // Quality gate: never accept DONE without at least one stable non-job proof ref.
            if status.eq_ignore_ascii_case("DONE")
                && !has_done_proof_ref(job_id, &job_priority, &refs)
            {
                effective_status = "CONTINUE";
                proof_gate_triggered = true;
                checkpoint_summary = if job_priority.eq_ignore_ascii_case("HIGH") {
                    "proof gate (HIGH): add CMD:/LINK:/FILE: to refs[] (or events.refs)".to_string()
                } else {
                    "proof gate: add non-job proof refs (CMD:/LINK:/FILE:/CARD-/TASK-/notes@seq) to refs[] (or events.refs)".to_string()
                };
                prior_summary = format!("{agent_summary}\n\n{checkpoint_summary}");
            }

            last_summary = Some(prior_summary);

            if effective_status.eq_ignore_ascii_case("DONE") {
                let _ = mcp.call_tool(
                    "jobs",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "jobs.complete",
                        "args": {
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "status": "DONE",
                            "summary": agent_summary,
                            "refs": refs,
                            "meta": { "runner": cfg.runner_id, "slice_index": slice_index }
                        }
                    }),
                );
                break 'job_loop;
            }
            if effective_status.eq_ignore_ascii_case("FAILED") {
                let _ = mcp.call_tool(
                    "jobs",
                    json!({
                        "workspace": cfg.workspace,
                        "op": "call",
                        "cmd": "jobs.complete",
                        "args": {
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "status": "FAILED",
                            "summary": agent_summary,
                            "refs": refs,
                            "meta": { "runner": cfg.runner_id, "slice_index": slice_index }
                        }
                    }),
                );
                break 'job_loop;
            }

            // CONTINUE: persist progress via events and run another slice.
            let _ = mcp.call_tool(
                "jobs",
                json!({
                    "workspace": cfg.workspace,
                    "op": "call",
                    "cmd": "jobs.report",
                    "args": {
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                        "kind": if proof_gate_triggered { "proof_gate" } else { "checkpoint" },
                        "message": if proof_gate_triggered {
                            format!("runner: proof gate (slice {}): {}", slice_index, checkpoint_summary)
                        } else {
                            format!("runner: slice {} CONTINUE: {}", slice_index, checkpoint_summary)
                        },
                        "percent": 0,
                        "refs": refs,
                        "meta": merged_meta(&base_meta, json!({
                            "runner": cfg.runner_id,
                            "slice_index": slice_index,
                            "proof_gate": effective_status.eq_ignore_ascii_case("CONTINUE") && status.eq_ignore_ascii_case("DONE"),
                            "slice_ms": now_ms().saturating_sub(slice_started_ms),
                            "step": {
                                "command": if proof_gate_triggered { "proof_gate" } else { "checkpoint" },
                                "result": checkpoint_summary.clone()
                            }
                        }))
                    }
                }),
            );
            slice_index = slice_index.saturating_add(1);
        }

        last_runner_beat_ms = now_ms();
        send_runner_heartbeat(&mut mcp, &cfg, "idle", None);

        if cfg.once {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
