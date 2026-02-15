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
  | Center (tabs: Architecture / Plan / Notes / Trace / Knowledge)
  | Inspector (details of selected task/step/node)
  + Command Palette (Cmd/Ctrl+K)
```

### Architecture tab (flagship lens)

Graph tab has two data sources:

- `Architecture lens` (default): synthesized map from anchors, knowledge keys/cards, tasks/plans, and reasoning graph fragments.
- `Reasoning graph`: raw branch/doc graph for the selected task.

Lens controls:

- `mode`: `combined | system | execution | reasoning | risk`
- `scope`: `workspace | plan | task`
- `time_window`: `all | 7d | 24h`
- `include draft`: toggles `v:draft` reasoning nodes

Inspector in architecture mode shows:

- summary capsule (coverage/proven ratio/risk count)
- selected node metadata (layer, risk/evidence, refs)
- provenance trail (`architecture_provenance_get`)

## Running locally

```bash
make viewer-install
make run-viewer
```

Linux notes:

- `make run-viewer` applies `WEBKIT_DISABLE_DMABUF_RENDERER=1` by default.
- If Wayland compositor disconnects, the launcher retries automatically with X11.
- You can force X11 manually:

```bash
make run-viewer-x11
```
