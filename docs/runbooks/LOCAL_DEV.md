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

## Viewer UI

Viewer assets are plain JS/CSS served from `crates/mcp/src/viewer/assets/` with no build step.

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

If you see many leftover `bm_mcp` processes (e.g., after a crashed TUI session), do a hard reset:

```bash
pkill -TERM -x bm_mcp || true
sleep 0.2
pkill -KILL -x bm_mcp || true

# Optional: clear shared-mode runtime sockets (safe; they are recreated on demand).
rm -f "${XDG_RUNTIME_DIR:-/tmp}/branchmind_mcp"/*.sock 2>/dev/null || true
```

Notes:

- Seeing a few `[bm_mcp] <defunct>` entries usually means an old parent process hasn’t reaped children yet; restarting that parent (or rebooting) clears them.
- Repo-local socket files under `.agents/mcp/.branchmind/*.sock` are safe to delete.

## Codex config (recommended)

```toml
[mcp_servers.branchmind]
command = "/home/amir/.local/bin/bm_mcp"
startup_timeout_sec = 120
args = ["--shared", "--toolset", "daily", "--agent-id", "auto", "--no-viewer"]
```

Notes:

- `--shared` is strongly recommended for multi-session stability.
- After rebuilding/reinstalling `bm_mcp`, fully restart the Codex TUI so it reloads MCP servers (otherwise a previously closed transport may stay “stuck” for the lifetime of that process).
