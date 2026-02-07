# Dependency Policy (current)

The long-term goal is a low-dependency, high-performance Rust MCP server suitable for IDE embedding.

## Rules

- Prefer Rust standard library for core logic.
- Any external crate must be justified:
  - why it is needed,
  - alternatives considered,
  - risks (supply chain, size, maintenance),
  - how it is isolated (core vs adapter).

## Policy decision (current)

- We use a **minimal audited dependency set** (not “0 deps strict”).
- `bm_core` remains **std-only**.
- External crates are allowed only in adapters (`bm_storage`, `bm_mcp`) with explicit justification.

If “0 deps strict” is required, replace these with in-house minimal implementations and document the trade-offs.

## Current usage (by crate)

### `bm_core` (domain core)

- *(no external dependencies)*

### `bm_storage` (persistence adapter)

- `rusqlite`: embedded SQLite persistence with transactional atomicity (task mutations + emitted events).
- `serde_json`: JSON payload/meta storage for documents, graph node meta, and MCP-compatible persistence shapes.

### `bm_mcp` (MCP adapter)

- `serde`: typed request parsing for MCP JSON-RPC envelopes.
- `serde_json`: JSON values + construction for tool inputs/outputs.
- `sha2`: deterministic in-process SHA-256 for local artifacts (proof receipts) without calling external programs (keeps `bm_mcp` “no arbitrary exec”).
- `time`: RFC3339 timestamps in agent-facing payloads.
- `nix` (unix-only): poll/select wrappers used to poll stdin with a timeout so hot reload can
  trigger even when the MCP client is idle (no manual restarts). Also used for POSIX signals
  in the local viewer process-takeover flow (no external `kill` command).

### `bm_viewer_tauri` (desktop viewer shell, optional)

- `tauri` (+ transitive WebView stack): cross-platform desktop window + tray wrapper that loads the
  local HTTP viewer UI from `http://127.0.0.1:${BRANCHMIND_VIEWER_PORT:-7331}`.
- `tauri-build`: build-time glue for Tauri config embedding.

Notes:

- This app is **not** part of the core MCP server and is kept out of the main Cargo workspace to
  avoid introducing GUI/system library requirements into CI for `bm_mcp`.
- Local HTTP viewer (optional) uses `std::net` only (no additional dependencies).
