# Local development

## Golden path

```bash
make check
make run-mcp
```

## Storage

- Default store directory: repo-local `.agents/mcp/.branchmind/` (derived from the nearest `.git` root)
- Override: `--storage-dir <path>`

## Workspace safety

- Default workspace: derived from repo root directory name.
- Workspace lock: enabled by default in the zero-arg DX run (`make run-mcp`) and available via `--workspace-lock` / `BRANCHMIND_WORKSPACE_LOCK`.

## Toolsets

- `daily`: minimal portal surface for everyday agent work (default).
- `full`: full parity surface.
- `core`: ultra-minimal tool surface.

Override with `--toolset` or `BRANCHMIND_TOOLSET`.

## Multi-session stability (shared mode)

For multiple concurrent agent CLIs (Codex/Claude/Gemini/etc.), prefer running the MCP server in `--shared` mode.
Each client session gets a small stdio proxy, which connects to (or spawns) a shared daemon.

Key properties:

- Daemon sockets are isolated by config **and build-compat fingerprint** (prevents “version tug-of-war”).
- Socket path auto-falls back to a short runtime dir when the repo path would exceed Unix `SUN_LEN`.
- Spawned daemons auto-exit after a short idle period to avoid process explosions.

## Debugging “Transport closed”

When a client reports `Transport closed`, check the repo-local store dir (default: `.agents/mcp/.branchmind/`) for:

- `branchmind_mcp_last_crash.txt` — best-effort crash report on panic / top-level error.
- `branchmind_mcp_last_session.txt` — last **proxy** session record (mode/last_method/last_error/exit), useful for handshake/framing issues.
- `branchmind_mcp_last_session_daemon.txt` — last **daemon** session record (args/cwd), useful when debugging shared socket bind/startup.

These files never include request bodies, only minimal metadata.

## Cleanup stray processes / sockets

Use the built-in shared reset command (same socket config resolution as normal startup):

```bash
make shared-reset
```

Notes:

- `--shared-reset` does **not** start stdio/shared/daemon loops; it only does best-effort daemon shutdown + stale socket unlink and prints a compact JSON report.
- `--shared-reset` by itself keeps the same auto defaults as zero-arg `make run-mcp`, so it targets the same default shared socket tag/path.
- If your client uses non-default flags/env (`--storage-dir`, `--toolset`, workspace/project guard), run the reset command with the same config so it targets the same socket.
- Global process-kill snippets should be kept as last-resort OS-level recovery only.

## Codex config (recommended)

```toml
[mcp_servers.branchmind]
command = "/home/amir/.local/bin/bm_mcp"
startup_timeout_sec = 120
args = ["--shared", "--toolset", "daily", "--agent-id", "auto"]
```

Notes:

- `--shared` is strongly recommended for multi-session stability.
- After rebuilding/reinstalling `bm_mcp`, fully restart the Codex TUI so it reloads MCP servers (otherwise a previously closed transport may stay “stuck” for the lifetime of that process).
