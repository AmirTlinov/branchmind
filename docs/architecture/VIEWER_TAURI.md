# Viewer (Tauri) — Architecture Notes

BranchMind includes an **optional** desktop viewer under `apps/viewer-tauri/`.
It is designed for *human situational awareness* without adding any HTTP server to `bm_mcp`.

## Non-goals

- No mutation (read-only only).
- No remote I/O / outbound network calls.
- No embedding into the core MCP runtime.

## Data source

- Opens a store directory using `bm_storage::SqliteStore::open_read_only`.
- Discovery scans common roots for a `branchmind_rust.db` in one of:
  - `<repo>/.agents/mcp/.branchmind/branchmind_rust.db` (repo-local default)
  - `<repo>/.branchmind_rust/branchmind_rust.db` (daemon default)
  - `<dir>/branchmind_rust.db` (legacy/dev)

Override discovery roots via:

```bash
export BRANCHMIND_VIEWER_SCAN_ROOTS="/abs/path/one;/abs/path/two"
```

## UI layout (IA)

Inspired by the “AI-first IDE Interface Design” reference:

```
Sidebar (projects/workspaces/tasks)
  | Center (tabs: Graph / Plan / Notes / Trace / Knowledge)
  | Inspector (details of selected task/step/node)
  + Command Palette (Cmd/Ctrl+K)
```

## Running locally

```bash
make viewer-install
make viewer-tauri-dev
```

