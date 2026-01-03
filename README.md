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

## Running (stdio MCP server)

The server is a stdio JSON-RPC MCP backend (no GUI/TUI).

Runtime flags:

- `--storage-dir <path>` — set the embedded store directory (default: `.branchmind_rust`).
- `--workspace <id>` — set the **default workspace** for portal tools (lets agents omit `workspace` in daily calls).
- `--toolset full|daily|core` — controls what is **advertised** via `tools/list`:
  - `full` (default): full parity surface (best for power users and compatibility).
  - `daily` (DX-first): **5-tool “portal” set** for everyday agent work (progressive disclosure).
  - `core` (ultra-minimal): **3-tool “golden path”** for the smallest possible tool surface.

`tools/list` also supports optional params `{ "toolset": "full|daily|core" }` to override the default for a single call.

Output formats (DX):

- Portal tools are **context-first**: they render a compact tagged line protocol (BM-L1) by default.
- The meaning of tags is defined once in docs/contracts and enforced by DX-DoD tests; portal outputs stay tag-light.
- Agent UX help lives in a dedicated `help` tool (protocol semantics + proof conventions) to avoid repeating boilerplate.

Environment:

- `BRANCHMIND_TOOLSET=full|daily|core` — same as `--toolset`, but useful for MCP clients that prefer env-based configuration.
- `BRANCHMIND_WORKSPACE=<id>` — same as `--workspace` (default workspace for portal tools).

Templates (DX):

- Built-in templates are discoverable via `tasks_templates_list`.
- `tasks_macro_start` / `tasks_bootstrap` support `template` as an alternative to explicit `steps`.
- `tasks_macro_start` supports optional `think` to seed the reasoning pipeline at task creation time.

Snapshots (DX):

- `tasks_snapshot` / `tasks_resume_super` include a small, versioned `capsule` intended for instant agent handoff (low-noise, survives aggressive `max_chars` trimming).
- `tasks_macro_close_step` accepts `checkpoints: "gate"` (criteria+tests) and `checkpoints: "all"` as compact shortcuts; it can also auto-pick the first open step when `path`/`step_id` is omitted (focus-first).
