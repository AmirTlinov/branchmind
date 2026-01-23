# BranchMind Rust (Unified Task+Reasoning MCP)

This repository is a Rust-first, MCP-only reimplementation that unifies the best parts of:

- task execution engines (decomposition, checkpoints, progress, “radar”)
- versioned reasoning memory (notes, diffs, merges, graphs, thinking traces)

The goal is to give AI agents a durable, low-noise “working memory” and an execution control tower that stays consistent across sessions.

## Start here

- `GOALS.md` — what “done” means for this project
- `PHILOSOPHY.md` — guiding principles (implementation-agnostic)
- `AGENTS.md` — map + development rules for AI agents
- `docs/contracts/OVERVIEW.md` — contract entrypoint (MCP tools + semantics)

Developer:

- Quick start: `docs/QUICK_START.md`
- Runbooks: `docs/runbooks/OVERVIEW.md`

## Running (stdio MCP server)

The server is a stdio JSON-RPC MCP backend (no GUI/TUI in the core).
An optional **local read-only HTTP viewer** can be enabled for human situational awareness.

Delegated work is tracked as `JOB-*` and executed out-of-process by `bm_runner` so `bm_mcp` can stay deterministic.

Runtime flags:

- `--storage-dir <path>` — set the embedded store directory (default: `.branchmind_rust`).
- `--workspace <id>` — set the **default workspace** for portal tools (lets agents omit `workspace` in daily calls).
- `--workspace-lock` — lock the server to the configured default workspace (rejects mismatched `workspace` to prevent accidental cross-project access).
- `--agent-id <id>` — set a default **actor id** used by the tasks subsystem (step leases) and some audit/meta fields when supported.
  - `--agent-id auto` creates (once) and reuses a stable default id stored in the embedded DB (survives restarts; reduces “forgot agent_id” drift).
  - Durable memory is **meaning-first and shared-by-default**; noise control uses visibility tags (`v:canon` / `v:draft`) plus explicit disclosure flags (`include_drafts` / `all_lanes` / `view="audit"`).
- `--toolset full|daily|core` — controls what is **advertised** via `tools/list`:
  - `full` (default): full parity surface (best for power users and compatibility).
  - `daily` (DX-first): a **small “portal” set** for everyday agent work (progressive disclosure).
  - `core` (ultra-minimal): **3-tool “golden path”** for the smallest possible tool surface.
- `--shared` — run a stdio proxy that connects to a shared local daemon (deduplicates processes across sessions).
- `--daemon` — run the shared local daemon on a Unix socket (no stdio).
- `--socket <path>` — override the Unix socket path (default: `<storage-dir>/branchmind_mcp.sock`).
- `--viewer` — enable the local read-only HTTP viewer (loopback-only).
- `--no-viewer` — disable the viewer (useful for headless runs).
- `--viewer-port <port>` — set the viewer port (default: `7331`).
- `--hot-reload` — (unix-only) auto-restart the running process via `exec` when the on-disk `bm_mcp` binary changes (dev DX).
- `--no-hot-reload` — disable hot reload.
- `--hot-reload-poll-ms <ms>` — override the hot reload polling interval (default: `1000`).

Hot reload defaults:

- In **auto-mode** (no args and no `BRANCHMIND_*` env), BranchMind uses shared proxy mode and enables hot reload by default.
- In **shared mode** (`--shared` / `BRANCHMIND_MCP_SHARED=1`), hot reload is enabled by default to keep long-lived local sessions aligned with rebuilds.
- To disable: pass `--no-hot-reload` or set `BRANCHMIND_HOT_RELOAD=0`.

Viewer note:

- The viewer is enabled by default at `http://127.0.0.1:7331` (loopback-only). Use `--no-viewer` or `BRANCHMIND_VIEWER=0` to disable.

Environment overrides:

- `BRANCHMIND_MCP_SHARED=1` — same as `--shared`.
- `BRANCHMIND_MCP_DAEMON=1` — same as `--daemon`.
- `BRANCHMIND_MCP_SOCKET=/path/to.sock` — same as `--socket`.
- `BRANCHMIND_PROJECT_GUARD=<value>` — same as `--project-guard`.
- `BRANCHMIND_VIEWER=1` — same as `--viewer`.
- `BRANCHMIND_VIEWER=0` — same as `--no-viewer`.
- `BRANCHMIND_VIEWER_PORT=<port>` — same as `--viewer-port`.

`tools/list` also supports optional params `{ "toolset": "full|daily|core" }` to override the default for a single call.

Output formats (DX):

- Portal tools are **context-first**: they render a compact tagged line protocol (BM-L1) by default.
- The meaning of tags is defined once in docs/contracts and enforced by DX-DoD tests; portal outputs stay tag-light.
- Agent UX help lives in a dedicated `help` tool (protocol semantics + proof conventions) to avoid repeating boilerplate.

Environment:

- `BRANCHMIND_TOOLSET=full|daily|core` — same as `--toolset`, but useful for MCP clients that prefer env-based configuration.
- `BRANCHMIND_WORKSPACE=<id>` — same as `--workspace` (default workspace for portal tools).
- `BRANCHMIND_WORKSPACE_LOCK=1` — same as `--workspace-lock`.
- `BRANCHMIND_PROJECT_GUARD=<value>` — same as `--project-guard`.
- `BRANCHMIND_AGENT_ID=<id>` — same as `--agent-id` (`auto` is supported).

Templates (DX):

- Built-in templates are discoverable via `tasks_templates_list`.
- `tasks_macro_start` / `tasks_bootstrap` support `template` as an alternative to explicit `steps`.
- `tasks_macro_start` supports optional `think` to seed the reasoning pipeline at task creation time.

Snapshots (DX):

- `tasks_snapshot` / `tasks_resume_super` include a small, versioned `capsule` intended for instant agent handoff (low-noise, survives aggressive `max_chars` trimming).
- `tasks_snapshot` defaults to `view="smart"` (relevance-first, cold archive). Use `view="explore"` for warm archive, `view="audit"` for cross-lane reads.
- `tasks_macro_close_step` accepts `checkpoints: "gate"` (criteria+tests) and `checkpoints: "all"` as compact shortcuts; it can also auto-pick the first open step when `path`/`step_id` is omitted (focus-first).

Multi-agent lanes (DX):

- Reasoning is **shared-by-default**; “don’t lose anything, don’t spam everything” is modeled via visibility tags:
  - `v:canon` — visible in smart views (the default for frontier + durable anchors).
  - `v:draft` — hidden by default (opt-in via `include_drafts=true` / `all_lanes=true` / `view="audit"`).
  - Default visibility when a card has no explicit visibility tag:
    - `decision|evidence|test|hypothesis|question|update` → `v:canon`
    - everything else (e.g. `note`) → `v:draft`
- `think_publish` promotes a card into the shared lane as a durable anchor (optionally pinned for smart views).

## Delegation runner (bm_runner)

`bm_mcp` never executes external programs; it only persists tasks/memory/jobs deterministically.

To make delegated jobs “real”, run the external runner:

- `bm_runner` polls `JOB-*`, claims work, runs a headless Codex session, and reports progress/results back via MCP.
- It supports long runs (up to 24h by default) via heartbeats, time-slices, and stale-job reclaim.

Quick loop:

1) Create a delegated task/job with `tasks_macro_delegate`.
2) Watch the inbox with `tasks_jobs_radar fmt=lines`.
3) Run `bm_runner` in the background to execute `JOB-*` and stream checkpoints into the job thread.

See:

- `docs/contracts/DELEGATION.md` (protocol, inbox format, proof gate).
- `docs/contracts/ANCHORS.md` (meaning map, anchor-scoped context).

## Development

Golden path:

```bash
make check
make run-mcp
```
