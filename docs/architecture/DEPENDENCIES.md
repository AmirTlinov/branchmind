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
  trigger even when the MCP client is idle (no manual restarts).

### `apps/viewer-tauri` (optional desktop viewer; outside the workspace)

This is a **read-only** desktop viewer built with **Tauri v2** (WebView runtime) + Vite/React.

Why it exists:

- Gives humans a way to understand *what/how/why* an agent is working (tasks/steps/notes/trace/graph),
  without turning `bm_mcp` into a web server.

Why it is acceptable (dependency budget):

- It is **isolated from the domain core** (`bm_core` remains std-only).
- It is **not** part of `cargo test --workspace` / `make check`.
- It performs **no outbound network calls** and reads stores using `open_read_only`.

Rust dependencies (viewer backend):

- `tauri` / `tauri-build`: desktop shell and IPC for local read-only commands.
- `serde` / `serde_json`: DTO serialization across IPC boundary.

Frontend dependencies (viewer UI):

- `react` + `vite`: UI runtime + build tooling.
- `zustand`: minimal predictable state container.
