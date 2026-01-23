#![forbid(unsafe_code)]

mod defaults;

use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug)]
struct RunnerConfig {
    workspace: String,
    storage_dir: PathBuf,
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
    skill_profile: String,
    skill_max_chars: usize,
}

fn usage() -> &'static str {
    "bm_runner — execute BranchMind JOB-* via headless Codex\n\n\
USAGE:\n\
  bm_runner [--storage-dir DIR] [--workspace WS] [--runner-id ID]\n\
            [--poll-ms MS] [--heartbeat-ms MS]\n\
            [--max-runtime-s S] [--slice-s S] [--slice-grace-s S]\n\
            [--stale-after-s S] [--once] [--dry-run]\n\
            [--mcp-bin PATH] [--codex-bin PATH]\n\
            [--skill-profile PROFILE] [--skill-max-chars N]\n\n\
NOTES:\n\
  - bm_mcp stays deterministic; this runner executes jobs out-of-process.\n\
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
    // The job lease is renewed via `tasks_jobs_report` heartbeats.
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

fn send_runner_heartbeat(
    mcp: &mut McpClient,
    cfg: &RunnerConfig,
    status: &str,
    active_job_id: Option<&str>,
) {
    let mut args = serde_json::Map::new();
    args.insert("workspace".to_string(), json!(cfg.workspace));
    args.insert("runner_id".to_string(), json!(cfg.runner_id));
    args.insert("status".to_string(), json!(status));
    args.insert("lease_ttl_ms".to_string(), json!(runner_lease_ttl_ms(cfg)));
    if let Some(job) = active_job_id {
        args.insert("active_job_id".to_string(), json!(job));
    }
    args.insert(
        "meta".to_string(),
        json!({
            "pid": std::process::id(),
            "poll_ms": cfg.poll_ms,
            "heartbeat_ms": cfg.heartbeat_ms
        }),
    );
    let _ = mcp.call_tool("tasks_runner_heartbeat", Value::Object(args));
}

fn has_non_job_proof_ref(job_id: &str, refs: &[String]) -> bool {
    refs.iter().any(|raw| {
        let r = raw.trim();
        if r.is_empty() {
            return false;
        }
        if r == job_id {
            return false;
        }
        // `JOB-*` (including `JOB-*@seq`) is navigation, not proof.
        if r.starts_with("JOB-") {
            return false;
        }
        // Anchors are meaning pointers; they do not prove completion.
        if r.starts_with("a:") {
            return false;
        }
        true
    })
}

fn push_unique_ref(out: &mut Vec<String>, value: String, seen: &mut HashSet<String>) {
    let v = value.trim();
    if v.is_empty() {
        return;
    }
    if seen.insert(v.to_string()) {
        out.push(v.to_string());
    }
}

fn canonicalize_ref(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // Keep these prefixes copy/paste friendly and stable.
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("cmd:") {
        return Some(format!("CMD:{}", &s[4..]));
    }
    if lower.starts_with("link:") {
        return Some(format!("LINK:{}", &s[5..]));
    }
    if lower.starts_with("card-") {
        return Some(format!("CARD-{}", &s[5..]));
    }
    if lower.starts_with("task-") {
        return Some(format!("TASK-{}", &s[5..]));
    }
    if lower.starts_with("notes@") {
        return Some(format!("notes@{}", &s[6..]));
    }

    Some(s.to_string())
}

fn salvage_proof_refs_from_text(text: &str) -> Vec<String> {
    // Goal: reduce false proof-gate CONTINUE when the delegated agent put proof-like refs
    // into summary/messages instead of top-level refs[].
    //
    // Strategy:
    // 1) Prefer full-line extraction for `CMD:` / `LINK:` (they contain spaces).
    // 2) Token extraction for CARD-/TASK-/notes@ references embedded in prose.
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let lower = l.to_ascii_lowercase();
        if let Some(norm) = lower
            .find("cmd:")
            .and_then(|idx| canonicalize_ref(&l[idx..]))
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }
        if let Some(norm) = lower
            .find("link:")
            .and_then(|idx| canonicalize_ref(&l[idx..]))
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }

        // Some agents write proof as markdown bullets without CMD:/LINK: prefixes.
        // We salvage only when the bullet strongly looks like a shell command
        // (avoid turning prose into fake proof).
        let mut bullet = l;
        if let Some(rest) = bullet.strip_prefix("- ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("* ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("• ") {
            bullet = rest.trim();
        }
        if let Some(rest) = bullet.strip_prefix("$ ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("> ") {
            bullet = rest.trim();
        }
        if !bullet.is_empty() {
            let b = bullet.to_ascii_lowercase();
            let looks_like_cmd = [
                "cargo ",
                "cargo",
                "pytest",
                "go test",
                "npm ",
                "pnpm ",
                "yarn ",
                "bun ",
                "make ",
                "just ",
                "git ",
                "rg ",
                "python ",
                "python3 ",
                "node ",
                "deno ",
                "docker ",
                "kubectl ",
                "helm ",
                "terraform ",
            ]
            .into_iter()
            .any(|p| b == p || b.starts_with(p));
            if looks_like_cmd {
                push_unique_ref(&mut out, format!("CMD: {bullet}"), &mut seen);
            }
        }
    }

    // Tokenize on common separators; keep it cheap and dependency-free.
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
            )
    }) {
        let token =
            raw.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '`'));
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            push_unique_ref(&mut out, format!("LINK: {token}"), &mut seen);
            continue;
        }
        if (lower.starts_with("card-") || lower.starts_with("task-") || lower.starts_with("notes@"))
            && let Some(norm) = canonicalize_ref(token)
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }
    }

    out
}

fn default_storage_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    defaults::default_storage_dir_from_start(&cwd)
}

fn default_workspace() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    defaults::default_workspace_from_start(&cwd)
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
    let skill_profile = skill_profile.unwrap_or_else(|| "strict".to_string());
    let skill_profile = normalize_skill_profile(&skill_profile)
        .ok_or("invalid --skill-profile (expected daily|strict|research|teamlead)")?;

    Ok(RunnerConfig {
        workspace,
        storage_dir,
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
        skill_profile,
        skill_max_chars,
    })
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    fn spawn(mcp_bin: &str, storage_dir: &Path, workspace: &str) -> Result<Self, String> {
        std::fs::create_dir_all(storage_dir)
            .map_err(|e| format!("failed to create storage dir: {e}"))?;

        let mut child = Command::new(mcp_bin)
            .arg("--shared")
            .arg("--storage-dir")
            .arg(storage_dir)
            .arg("--toolset")
            .arg("full")
            .arg("--workspace")
            .arg(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn bm_mcp ({mcp_bin}): {e}"))?;

        let stdin = child.stdin.take().ok_or("bm_mcp stdin unavailable")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("bm_mcp stdout unavailable")?);

        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        })
    }

    fn send(&mut self, req: Value) -> Result<(), String> {
        writeln!(self.stdin, "{req}").map_err(|e| format!("write request failed: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("flush failed: {e}"))?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| format!("read response failed: {e}"))?;
        if line.trim().is_empty() {
            return Err("empty response line from bm_mcp".to_string());
        }
        serde_json::from_str(&line).map_err(|e| format!("parse response json failed: {e}"))
    }

    fn request(&mut self, req: Value) -> Result<Value, String> {
        self.send(req)?;
        self.recv()
    }

    fn initialize(&mut self) -> Result<(), String> {
        let init_id = self.next_id;
        self.next_id += 1;
        let _ = self.request(json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "bm_runner", "version": "0.1.0" }
            }
        }))?;
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))?;
        Ok(())
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))?;

        if resp.get("error").is_some() {
            let msg = resp
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("mcp error");
            return Err(format!("{name} failed: {msg}"));
        }

        let text = resp
            .get("result")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.get(0))
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{name} missing result.content[0].text"))?;

        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            // Most BranchMind tools return an AI-envelope JSON object:
            // { success, intent, result, warnings, ... }.
            // The runner operates on the inner `result` payload.
            if parsed
                .get("success")
                .and_then(|v| v.as_bool())
                .is_some_and(|ok| !ok)
            {
                let msg = parsed
                    .get("error")
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool error");
                return Err(format!("{name} failed: {msg}"));
            }
            if let Some(inner) = parsed.get("result") {
                return Ok(inner.clone());
            }
            return Ok(parsed);
        }

        Ok(Value::String(text.to_string()))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn value_as_str(v: &Value) -> Option<&str> {
    v.as_str().map(|s| s.trim()).filter(|s| !s.is_empty())
}

fn value_as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
}

fn sanitize_single_line(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    let sanitized = sanitize_single_line(text).trim().to_string();
    if sanitized.chars().count() <= max_chars {
        return sanitized;
    }
    let mut out = String::new();
    for (i, ch) in sanitized.chars().enumerate() {
        if i >= max_chars.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn render_job_thread(opened: &Value) -> String {
    const MAX_EVENTS: usize = 12;
    const MAX_MESSAGE_CHARS: usize = 160;
    const MAX_REFS: usize = 3;

    let Some(events) = opened.get("events").and_then(|v| v.as_array()) else {
        return String::new();
    };

    // events are newest-first; filter, then show oldest->newest for readability.
    let mut picked: Vec<&Value> = Vec::new();
    for ev in events {
        let kind = ev.get("kind").and_then(value_as_str).unwrap_or("");
        if kind.eq_ignore_ascii_case("heartbeat") {
            continue;
        }
        let msg = ev.get("message").and_then(value_as_str).unwrap_or("");
        if msg.to_ascii_lowercase().starts_with("runner:") {
            continue;
        }
        picked.push(ev);
        if picked.len() >= MAX_EVENTS {
            break;
        }
    }
    picked.reverse();

    let mut lines: Vec<String> = Vec::new();
    for ev in picked {
        let kind = ev.get("kind").and_then(value_as_str).unwrap_or("event");
        let message = truncate_for_prompt(
            ev.get("message").and_then(value_as_str).unwrap_or("-"),
            MAX_MESSAGE_CHARS,
        );

        let mut refs: Vec<String> = ev
            .get("refs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(MAX_REFS)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        refs.retain(|r| !r.starts_with("JOB-"));

        if refs.is_empty() {
            lines.push(format!("- {kind}: {message}"));
        } else {
            lines.push(format!("- {kind}: {message} (refs: {})", refs.join(", ")));
        }
    }

    if lines.is_empty() {
        "(no messages)".to_string()
    } else {
        lines.join("\n")
    }
}

fn build_subagent_prompt(
    cfg: &RunnerConfig,
    job_id: &str,
    job_prompt: &str,
    slice_context: &str,
    skill_pack: &str,
) -> String {
    let skill_section = if skill_pack.trim().is_empty() {
        String::new()
    } else {
        format!("SKILL PACK (bounded):\n{skill_pack}\n\n")
    };

    format!(
        "You are a delegated coding agent.\n\
You MUST return a single JSON object that matches the provided output schema.\n\
Do not include extra keys.\n\
Keep summary short.\n\
Put stable BranchMind refs into refs[] (TASK-*, JOB-*, CARD-*, notes@seq, a:*).\n\
Proof gate: status=\"DONE\" requires at least one non-job ref (e.g., CARD-* / TASK-* / notes@seq / LINK:/CMD:).\n\
Always include events[] in the final JSON (use [] if none).\n\
Each events[] item MUST include all keys: kind, message, percent, refs (use percent=0 if unknown; refs=[] if none).\n\
\n\
EXAMPLE OUTPUT (valid JSON):\n\
{{\"status\":\"DONE\",\"summary\":\"...\",\"refs\":[\"CMD: cargo test -q\",\"CARD-123\",\"JOB-001\"],\"events\":[{{\"kind\":\"progress\",\"message\":\"...\",\"percent\":10,\"refs\":[\"JOB-001\"]}}]}}\n\
IMPORTANT: Put proof refs (CMD:/LINK:/CARD-/TASK-/notes@seq) into refs[] (or events[].refs). Do not bury them only in summary.\n\
\n\
{skill_section}\
FEEDBACK LOOP (low-noise):\n\
- workspace: {workspace}\n\
- job: {job}\n\
If the MCP tool `tasks_jobs_report` is available, send 1–3 short updates while you work:\n\
- kind: progress|checkpoint|question\n\
- message: short, no logs\n\
- percent: integer (0 if unknown)\n\
- refs: stable ids (CARD-*/TASK-*/notes@seq/a:*)\n\
If you cannot call tools, emit the same updates in the final JSON field `events`.\n\
\n\
MANAGER CONTROL:\n\
- The manager may send messages via `tasks_jobs_message`.\n\
- Read the JOB THREAD and follow the latest manager instruction.\n\
\n\
TIME-SLICE RULE:\n\
If you cannot fully finish within this slice, return status=\"CONTINUE\" with (a) what you did and (b) the next best action.\n\
\n\
JOB SPEC:\n{job_prompt}\n\n\
{slice_context}\n",
        workspace = cfg.workspace,
        job = job_id,
        job_prompt = job_prompt,
        slice_context = slice_context,
        skill_section = skill_section
    )
}

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

fn ensure_codex_schema(tmp_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(tmp_dir).map_err(|e| format!("tmp dir create failed: {e}"))?;
    let schema_path = tmp_dir.join("output_schema.json");

    // Minimal structured contract: the runner only needs status + summary + stable refs.
    // We allow CONTINUE so multi-hour jobs can be time-sliced safely.
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "status": { "type": "string", "enum": ["DONE", "FAILED", "CONTINUE"] },
            "summary": { "type": "string" },
            "refs": { "type": "array", "items": { "type": "string" } },
            "events": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": { "type": "string" },
                        "message": { "type": "string" },
                        "percent": { "type": "integer" },
                        "refs": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["kind", "message", "percent", "refs"]
                }
            }
        },
        "required": ["events", "refs", "status", "summary"]
    });

    std::fs::write(
        &schema_path,
        serde_json::to_vec_pretty(&schema).unwrap_or_default(),
    )
    .map_err(|e| format!("write schema failed: {e}"))?;
    Ok(schema_path)
}

fn spawn_codex_exec(
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

fn read_codex_output(out_path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(out_path).map_err(|e| format!("read codex output failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse codex json failed: {e}"))
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
    let schema_path = ensure_codex_schema(&tmp_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });

    loop {
        // Prefer queued jobs. If none, attempt to reclaim a stale RUNNING job.
        let queued = mcp.call_tool(
            "tasks_jobs_list",
            json!({
                "workspace": cfg.workspace,
                "status": "QUEUED",
                "limit": 1,
                "max_chars": 4000
            }),
        )?;
        let candidate = if let Some(job) = first_job_from_list(&queued) {
            Some(job)
        } else {
            // Bounded scan for stale RUNNING jobs (ralf-loop recovery).
            let running = mcp.call_tool(
                "tasks_jobs_list",
                json!({
                    "workspace": cfg.workspace,
                    "status": "RUNNING",
                    "limit": 200,
                    "max_chars": 12000
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
                "workspace": cfg.workspace,
                "job": job_id,
                "runner_id": cfg.runner_id,
                "allow_stale": true,
                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg)
            })
        } else {
            json!({
                "workspace": cfg.workspace,
                "job": job_id,
                "runner_id": cfg.runner_id,
                "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg)
            })
        };
        let claim = mcp.call_tool("tasks_jobs_claim", claim_args);
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
            "workspace": cfg.workspace,
            "job": job_id,
            "include_prompt": true,
            "include_events": false,
            "max_events": 0,
            "include_meta": true,
            "max_chars": 8000
        });
        let open = mcp.call_tool("tasks_jobs_open", open_args).or_else(|_| {
            mcp.call_tool(
                "tasks_jobs_open",
                json!({
                    "workspace": cfg.workspace,
                    "job": job_id,
                    "include_prompt": true,
                    "include_events": false,
                    "max_events": 0,
                    "max_chars": 8000
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
                "skill",
                json!({
                    "profile": selected_skill_profile,
                    "max_chars": selected_skill_max_chars
                }),
            ) {
                Ok(Value::String(s)) => s,
                Ok(other) => other.to_string(),
                Err(_) => String::new(),
            }
        };

        let anchor_snapshot = if let Some(anchor_id) = anchor_id.as_deref() {
            match mcp.call_tool(
                "anchor_snapshot",
                json!({
                    "workspace": cfg.workspace,
                    "anchor": anchor_id,
                    "include_drafts": false,
                    "limit": 20,
                    "max_chars": 2500
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
                "tasks_jobs_report",
                json!({
                    "workspace": cfg.workspace,
                    "job": job_id,
                    "runner_id": cfg.runner_id,
                    "claim_revision": claim_revision,
                    "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                    "kind": "checkpoint",
                    "message": "dry-run: claimed and completing immediately",
                    "percent": 0,
                    "refs": [ job_id ],
                    "meta": { "dry_run": true }
                }),
            );
            let _ = mcp.call_tool(
                "tasks_jobs_complete",
                json!({
                    "workspace": cfg.workspace,
                    "job": job_id,
                    "runner_id": cfg.runner_id,
                    "claim_revision": claim_revision,
                    "status": "DONE",
                    "summary": "dry-run complete",
                    "refs": [ job_id ],
                    "meta": { "dry_run": true }
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
                    "tasks_jobs_complete",
                    json!({
                        "workspace": cfg.workspace,
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
                    }),
                );
                break 'job_loop;
            }

            let snapshot_lines = if let Some(task_id) = task_id.as_deref() {
                match mcp.call_tool(
                    "tasks_snapshot",
                    json!({
                        "workspace": cfg.workspace,
                        "task": task_id,
                        "fmt": "lines",
                        "refs": true,
                        "max_chars": 7000
                    }),
                ) {
                    Ok(Value::String(s)) => s,
                    Ok(other) => other.to_string(),
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            };

            let job_thread = match mcp.call_tool(
                "tasks_jobs_open",
                json!({
                    "workspace": cfg.workspace,
                    "job": job_id,
                    "include_prompt": false,
                    "include_events": true,
                    "max_events": 40,
                    "max_chars": 6000
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

            let full_prompt =
                build_subagent_prompt(&cfg, job_id, &prompt, &slice_context, &skill_pack);

            let _ = mcp.call_tool(
                "tasks_jobs_report",
                json!({
                    "workspace": cfg.workspace,
                    "job": job_id,
                    "runner_id": cfg.runner_id,
                    "claim_revision": claim_revision,
                    "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                    "kind": "checkpoint",
                    "message": format!("runner: slice {} started{}", slice_index, if is_reclaim { " (reclaim)" } else { "" }),
                    "percent": 0,
                    "refs": [ job_id ],
                    "meta": {
                        "runner": cfg.runner_id,
                        "slice_index": slice_index,
                        "reclaim": is_reclaim,
                        "slice_s": cfg.slice_s,
                        "slice_grace_s": cfg.slice_grace_s,
                        "heartbeat_ms": cfg.heartbeat_ms
                    }
                }),
            );

            let out_path = tmp_dir.join(format!("job_{job_id}_slice_{slice_index}.json"));
            let stderr_path = tmp_dir.join(format!("job_{job_id}_slice_{slice_index}.stderr"));
            let mut child = match spawn_codex_exec(
                &cfg,
                &schema_path,
                &out_path,
                &stderr_path,
                &full_prompt,
            ) {
                Ok(c) => c,
                Err(err) => {
                    failures = failures.saturating_add(1);
                    let _ = mcp.call_tool(
                        "tasks_jobs_report",
                        json!({
                            "workspace": cfg.workspace,
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                            "kind": "error",
                            "message": format!("runner: spawn failed (failures={failures}): {err}"),
                            "percent": 0,
                            "refs": [ job_id ],
                            "meta": { "runner": cfg.runner_id, "slice_index": slice_index, "failures": failures }
                        }),
                    );
                    if failures >= cfg.max_failures {
                        let _ = mcp.call_tool(
                            "tasks_jobs_complete",
                            json!({
                                "workspace": cfg.workspace,
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "status": "FAILED",
                                "summary": "runner: spawn failures exceeded",
                                "refs": [ job_id ],
                                "meta": { "runner": cfg.runner_id, "failures": failures }
                            }),
                        );
                        break 'job_loop;
                    }
                    slice_index = slice_index.saturating_add(1);
                    continue 'job_loop;
                }
            };

            let slice_started_ms = now_ms();
            let slice_soft_ms = (cfg.slice_s as i64).saturating_mul(1000);
            let slice_hard_ms =
                slice_soft_ms.saturating_add((cfg.slice_grace_s as i64).saturating_mul(1000));
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
                if now.saturating_sub(last_beat_ms) >= cfg.heartbeat_ms as i64 {
                    last_beat_ms = now;
                    send_runner_heartbeat(&mut mcp, &cfg, "live", Some(job_id));
                    let beat = mcp.call_tool(
                        "tasks_jobs_report",
                        json!({
                            "workspace": cfg.workspace,
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                            "kind": "heartbeat",
                            "message": format!("runner: heartbeat (slice {})", slice_index),
                            "percent": 0,
                            "refs": [ job_id ],
                            "meta": {
                                "runner": cfg.runner_id,
                                "slice_index": slice_index,
                                "uptime_s": now.saturating_sub(job_started_ms) / 1000
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
                let exit_meta = exit.clone();
                let _ = mcp.call_tool(
                        "tasks_jobs_report",
                        json!({
                            "workspace": cfg.workspace,
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                            "kind": "error",
                            "message": format!(
                                "runner: codex exec failed (exit={exit}, killed={killed}, failures={failures}): {stderr_snip}"
                            ),
                            "percent": 0,
                            "refs": [ job_id ],
                            "meta": {
                                "runner": cfg.runner_id,
                                "slice_index": slice_index,
                                "killed": killed,
                                "failures": failures,
                                "exit": exit_meta
                            }
                        }),
                    );
                if failures >= cfg.max_failures {
                    let _ = mcp.call_tool(
                        "tasks_jobs_complete",
                        json!({
                            "workspace": cfg.workspace,
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "status": "FAILED",
                            "summary": "runner: codex exec failures exceeded",
                            "refs": [ job_id ],
                            "meta": { "runner": cfg.runner_id, "failures": failures }
                        }),
                    );
                    break 'job_loop;
                }
                slice_index = slice_index.saturating_add(1);
                continue 'job_loop;
            }

            let outcome = read_codex_output(&out_path);
            let v = match outcome {
                Ok(v) => v,
                Err(err) => {
                    failures = failures.saturating_add(1);
                    let stderr_snip = read_stderr_snippet(&stderr_path, 0, 400);
                    let exit = exit_status
                        .as_ref()
                        .map(format_exit_status)
                        .unwrap_or_else(|| "-".to_string());
                    let exit_meta = exit.clone();
                    let _ = mcp.call_tool(
                        "tasks_jobs_report",
                        json!({
                            "workspace": cfg.workspace,
                            "job": job_id,
                            "runner_id": cfg.runner_id,
                            "claim_revision": claim_revision,
                            "lease_ttl_ms": job_claim_lease_ttl_ms(&cfg),
                            "kind": "error",
                            "message": format!("runner: codex output unreadable (exit={exit}, killed={killed}, failures={failures}): {err}; stderr: {stderr_snip}"),
                            "percent": 0,
                            "refs": [ job_id ],
                            "meta": { "runner": cfg.runner_id, "slice_index": slice_index, "killed": killed, "failures": failures, "exit": exit_meta }
                        }),
                    );
                    if failures >= cfg.max_failures {
                        let _ = mcp.call_tool(
                            "tasks_jobs_complete",
                            json!({
                                "workspace": cfg.workspace,
                                "job": job_id,
                                "runner_id": cfg.runner_id,
                                "claim_revision": claim_revision,
                                "status": "FAILED",
                                "summary": "runner: output failures exceeded",
                                "refs": [ job_id ],
                                "meta": { "runner": cfg.runner_id, "failures": failures }
                            }),
                        );
                        break 'job_loop;
                    }
                    slice_index = slice_index.saturating_add(1);
                    continue 'job_loop;
                }
            };

            let status = v.get("status").and_then(value_as_str).unwrap_or("FAILED");
            let agent_summary = v
                .get("summary")
                .and_then(value_as_str)
                .unwrap_or("-")
                .to_string();
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
                    report_args.insert("workspace".to_string(), json!(cfg.workspace));
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
                    let _ = mcp.call_tool("tasks_jobs_report", Value::Object(report_args));
                }
            }
            // Merge event refs into job refs (prevents false proof-gate CONTINUE).
            for r in event_refs_union {
                if !refs.iter().any(|existing| existing == &r) {
                    refs.push(r);
                }
            }

            failures = 0;
            let mut prior_summary = agent_summary.clone();
            last_refs = refs.clone();

            let mut effective_status = status;
            let mut checkpoint_summary = agent_summary.clone();
            let mut proof_gate_triggered = false;

            // Quality gate: never accept DONE without at least one stable non-job proof ref.
            if status.eq_ignore_ascii_case("DONE") && !has_non_job_proof_ref(job_id, &refs) {
                effective_status = "CONTINUE";
                proof_gate_triggered = true;
                checkpoint_summary = "proof gate: add CMD:/LINK:/CARD-/TASK-/notes@seq to refs[] (or emit them in events.refs)".to_string();
                prior_summary = format!("{agent_summary}\n\n{checkpoint_summary}");
            }

            last_summary = Some(prior_summary);

            if effective_status.eq_ignore_ascii_case("DONE") {
                let _ = mcp.call_tool(
                    "tasks_jobs_complete",
                    json!({
                        "workspace": cfg.workspace,
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "status": "DONE",
                        "summary": agent_summary,
                        "refs": refs,
                        "meta": { "runner": cfg.runner_id, "slice_index": slice_index }
                    }),
                );
                break 'job_loop;
            }
            if effective_status.eq_ignore_ascii_case("FAILED") {
                let _ = mcp.call_tool(
                    "tasks_jobs_complete",
                    json!({
                        "workspace": cfg.workspace,
                        "job": job_id,
                        "runner_id": cfg.runner_id,
                        "claim_revision": claim_revision,
                        "status": "FAILED",
                        "summary": agent_summary,
                        "refs": refs,
                        "meta": { "runner": cfg.runner_id, "slice_index": slice_index }
                    }),
                );
                break 'job_loop;
            }

            // CONTINUE: persist progress via events and run another slice.
            let _ = mcp.call_tool(
                "tasks_jobs_report",
                json!({
                    "workspace": cfg.workspace,
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
                    "meta": {
                        "runner": cfg.runner_id,
                        "slice_index": slice_index,
                        "proof_gate": effective_status.eq_ignore_ascii_case("CONTINUE") && status.eq_ignore_ascii_case("DONE"),
                        "slice_ms": now_ms().saturating_sub(slice_started_ms)
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
mod tests {
    use super::*;
    use serde_json::json;

    fn sorted_property_keys(schema: &Value) -> Vec<String> {
        let mut keys = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        keys.sort();
        keys
    }

    fn required_keys(schema: &Value) -> Vec<String> {
        schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[test]
    fn codex_output_schema_required_matches_properties() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "bm_runner_schema_test_{}_{}",
            std::process::id(),
            now_ms()
        ));
        let schema_path = ensure_codex_schema(&tmp_dir).expect("ensure_codex_schema");
        let text = std::fs::read_to_string(&schema_path).expect("read schema");
        let schema: Value = serde_json::from_str(&text).expect("parse schema");

        assert_eq!(required_keys(&schema), sorted_property_keys(&schema));

        let events_item = schema
            .get("properties")
            .and_then(|v| v.get("events"))
            .and_then(|v| v.get("items"))
            .cloned()
            .expect("events.items");
        assert_eq!(
            required_keys(&events_item),
            sorted_property_keys(&events_item)
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn proof_gate_requires_non_job_refs() {
        let job = "JOB-001";
        assert!(!has_non_job_proof_ref(job, &[]), "empty refs must fail");
        assert!(
            !has_non_job_proof_ref(job, &[job.to_string()]),
            "job id alone is navigation, not proof"
        );
        assert!(
            !has_non_job_proof_ref(job, &["JOB-001@2".to_string()]),
            "job event refs are navigation, not proof"
        );
        assert!(
            !has_non_job_proof_ref(job, &["a:core".to_string()]),
            "anchors are meaning pointers, not proof"
        );
        assert!(has_non_job_proof_ref(job, &["CARD-1".to_string()]));
        assert!(has_non_job_proof_ref(job, &["TASK-123".to_string()]));
        assert!(has_non_job_proof_ref(job, &["notes@42".to_string()]));
        assert!(has_non_job_proof_ref(
            job,
            &["LINK: ci-run-123".to_string()]
        ));
        assert!(has_non_job_proof_ref(job, &["CMD: cargo test".to_string()]));
    }

    #[test]
    fn salvage_proof_refs_extracts_cmd_and_link_lines() {
        let text = "did stuff\ncmd: cargo test -q\nmore\nLINK: ci-run-123\n";
        let refs = salvage_proof_refs_from_text(text);
        assert!(refs.contains(&"CMD: cargo test -q".to_string()));
        assert!(refs.contains(&"LINK: ci-run-123".to_string()));
    }

    #[test]
    fn salvage_proof_refs_extracts_embedded_card_task_notes_tokens() {
        let text = "see CARD-123 and task-456; notes@42. also: JOB-001";
        let refs = salvage_proof_refs_from_text(text);
        assert!(refs.contains(&"CARD-123".to_string()));
        assert!(refs.contains(&"TASK-456".to_string()));
        assert!(refs.contains(&"notes@42".to_string()));
        assert!(
            !refs.iter().any(|r| r == "JOB-001"),
            "job ids are not proof refs"
        );
    }

    #[test]
    fn salvage_proof_refs_extracts_plain_urls_as_link() {
        let text = "CI: https://example.com/ci/run/123 (green)";
        let refs = salvage_proof_refs_from_text(text);
        assert!(refs.contains(&"LINK: https://example.com/ci/run/123".to_string()));
    }

    #[test]
    fn salvage_proof_refs_extracts_markdown_bullet_commands_carefully() {
        let text = "- cargo test -q\n- Updated docs\n- $ pytest -q\n";
        let refs = salvage_proof_refs_from_text(text);
        assert!(refs.contains(&"CMD: cargo test -q".to_string()));
        assert!(refs.contains(&"CMD: pytest -q".to_string()));
        assert!(
            !refs.iter().any(|r| r.contains("Updated docs")),
            "should not treat prose bullets as commands"
        );
    }

    #[test]
    fn normalize_skill_profile_accepts_known_profiles() {
        assert_eq!(normalize_skill_profile("daily").as_deref(), Some("daily"));
        assert_eq!(normalize_skill_profile("STRICT").as_deref(), Some("strict"));
        assert_eq!(
            normalize_skill_profile(" research ").as_deref(),
            Some("research")
        );
        assert_eq!(
            normalize_skill_profile("teamlead").as_deref(),
            Some("teamlead")
        );
        assert_eq!(normalize_skill_profile("unknown"), None);
    }

    #[test]
    fn build_subagent_prompt_includes_skill_pack_when_present() {
        let cfg = RunnerConfig {
            workspace: "ws".to_string(),
            storage_dir: PathBuf::from("."),
            runner_id: "r".to_string(),
            poll_ms: 1000,
            heartbeat_ms: 1000,
            max_runtime_s: 10,
            slice_s: 1,
            slice_grace_s: 0,
            stale_after_s: 1,
            max_failures: 1,
            once: true,
            dry_run: true,
            mcp_bin: "bm_mcp".to_string(),
            codex_bin: "codex".to_string(),
            skill_profile: "strict".to_string(),
            skill_max_chars: 1200,
        };
        let prompt = build_subagent_prompt(
            &cfg,
            "JOB-1",
            "do thing",
            "CTX",
            "skill profile=strict version=0.1.0\n[CORE LOOP]\n...",
        );
        assert!(prompt.contains("SKILL PACK (bounded):"));
        assert!(prompt.contains("skill profile=strict"));
        assert!(prompt.contains("JOB SPEC:\n"));
    }

    #[test]
    fn skill_selection_prefers_job_meta_then_kind_then_default() {
        let cfg = RunnerConfig {
            workspace: "ws".to_string(),
            storage_dir: PathBuf::from("."),
            runner_id: "r".to_string(),
            poll_ms: 1000,
            heartbeat_ms: 1000,
            max_runtime_s: 10,
            slice_s: 1,
            slice_grace_s: 0,
            stale_after_s: 1,
            max_failures: 1,
            once: true,
            dry_run: true,
            mcp_bin: "bm_mcp".to_string(),
            codex_bin: "codex".to_string(),
            skill_profile: "strict".to_string(),
            skill_max_chars: 1200,
        };

        let meta = json!({"skill_profile":"daily"});
        assert_eq!(
            select_skill_profile(Some("codex_cli"), Some(&meta), &cfg),
            "daily".to_string()
        );

        let no_meta = json!(null);
        assert_eq!(
            select_skill_profile(Some("research_probe"), Some(&no_meta), &cfg),
            "research".to_string()
        );

        assert_eq!(select_skill_profile(None, None, &cfg), "strict".to_string());
    }

    #[test]
    fn skill_budget_can_be_overridden_or_disabled_by_job_meta() {
        let cfg = RunnerConfig {
            workspace: "ws".to_string(),
            storage_dir: PathBuf::from("."),
            runner_id: "r".to_string(),
            poll_ms: 1000,
            heartbeat_ms: 1000,
            max_runtime_s: 10,
            slice_s: 1,
            slice_grace_s: 0,
            stale_after_s: 1,
            max_failures: 1,
            once: true,
            dry_run: true,
            mcp_bin: "bm_mcp".to_string(),
            codex_bin: "codex".to_string(),
            skill_profile: "strict".to_string(),
            skill_max_chars: 1200,
        };

        let meta = json!({"skill_max_chars": 500});
        assert_eq!(select_skill_max_chars(Some(&meta), &cfg), 500);

        let meta_off = json!({"skill_max_chars": 0});
        assert_eq!(select_skill_max_chars(Some(&meta_off), &cfg), 0);

        assert_eq!(select_skill_max_chars(None, &cfg), 1200);
    }
}
