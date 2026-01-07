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
- `--workspace-lock` — lock the server to the configured default workspace (rejects mismatched `workspace` to prevent accidental cross-project access).
- `--agent-id <id>` — set a default agent id for multi-agent lanes (used when `agent_id` is omitted in tools that support it).
  - `--agent-id auto` creates (once) and reuses a stable default id stored in the embedded DB (survives restarts; reduces “forgot agent_id” drift).
- `--toolset full|daily|core` — controls what is **advertised** via `tools/list`:
  - `full` (default): full parity surface (best for power users and compatibility).
  - `daily` (DX-first): **5-tool “portal” set** for everyday agent work (progressive disclosure).
  - `core` (ultra-minimal): **3-tool “golden path”** for the smallest possible tool surface.
- `--project-guard <value>` — enforce a workspace-bound guard value stored in the DB (mismatch becomes a typed error; prevents opening a store belonging to a different project).

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

- Reasoning writes (`think_*`, `notes_commit`, `macro_branch_note`) accept optional `agent_id` to stamp artifacts into an agent lane.
- Relevance-first views (`tasks_resume_super` smart/focus_only, `think_watch`) filter out other agents’ lanes by default (shared + “my lane”).
- `think_publish` promotes a card into the shared lane as a durable anchor (optionally pinned for smart views).
