# Hot Reload (DX, local-only)

BranchMind supports a **local dev hot reload** workflow: when the on-disk `bm_mcp` binary is
rebuilt/replaced, already running MCP server processes will automatically upgrade.

## Goals

- Keep long-lived agent sessions usable during active development.
- Avoid MCP startup failures (no broken pipes during initialize).
- Stay deterministic and safe (no outbound network; no arbitrary program execution features added).

## Mechanism

- **Unix-only**: the process performs a best-effort `exec` into the new binary when it detects that
  the on-disk executable changed.
- **Stdio safety**: `exec` is only attempted at a safe point when the stdio `BufReader` has no
  prefetched bytes (`buffer().is_empty()`), to avoid losing already-read bytes.

## Controls

- Enable: `--hot-reload` or `BRANCHMIND_HOT_RELOAD=1`
- Disable: `--no-hot-reload` or `BRANCHMIND_HOT_RELOAD=0`
- Poll interval: `--hot-reload-poll-ms <ms>` or `BRANCHMIND_HOT_RELOAD_POLL_MS=<ms>`

Default:

- In `--shared` proxy mode, hot reload is **enabled by default** (DX: avoids stale daemons after rebuilds).
- In `--daemon` mode (and plain stdio), hot reload is **disabled by default** for stability.
