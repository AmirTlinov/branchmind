# Contracts — Local Viewer (optional, v1)

This document defines the **optional local read-only HTTP viewer** surface.
It is not part of the MCP tool surface, but it is still a contract.

## Scope

- Local-only, loopback-bound (`127.0.0.1`), read-only.
- No outbound network calls.
- Feature-flagged (disable via `--no-viewer` / `BRANCHMIND_VIEWER=0`).
- Must not be required for MCP correctness (viewer startup failures must not break MCP startup).

Default behavior:

- The viewer is enabled by default on `127.0.0.1:7331` (loopback-only) for **session** modes
  (stdio + shared proxy).
- In `--daemon` mode the viewer is **disabled by default** to prevent a long-lived background
  process from keeping `:7331` occupied after the calling session exits.

Enabling/disabling:

- Session viewer:
  - Enable: `--viewer` or `BRANCHMIND_VIEWER=1`
  - Disable: `--no-viewer` or `BRANCHMIND_VIEWER=0`
- Daemon viewer (opt-in):
  - Enable: `--viewer` or `BRANCHMIND_VIEWER_DAEMON=1`
  - Disable: `--no-viewer` or `BRANCHMIND_VIEWER_DAEMON=0`

Notes:

- In shared mode, the proxy owns the session-scoped viewer and **always** spawns daemons with
  `--no-viewer`. This ensures the viewer lifetime matches the proxy process lifetime.

## Endpoints

### `GET /`

Returns the static HTML shell for the viewer UI.

### `GET /app.css`

Returns the UI stylesheet.

### `GET /app.js`

Returns the UI script (runtime JS).

### `GET /api/about`

Returns viewer/runtime identity hints for UX:

```json
{
  "fingerprint": "string",
  "project_guard": "string|null",
  "workspace_default": "string|null",
  "workspace_recommended": "string"
}
```

Notes:

- `workspace_recommended` is derived from the repo root directory name (sanitized).
- `fingerprint` identifies the exact binary build (used for session safety + upgrade/takeover heuristics).
- This endpoint is read-only and does not touch the store.
- Optional query params:
  - `project=<project_guard>`: view another active project (see `/api/projects`).

### `POST /api/internal/shutdown` (internal)

Best-effort internal endpoint used for local dev upgrade ergonomics.

- Local-only.
- Does **not** mutate the store (viewer-only).
- Not part of the public UX surface (no UI button).

Request:

```json
{ "fingerprint": "string" }
```

The fingerprint must match the current viewer's `/api/about.fingerprint` to avoid accidental
cross-session shutdown.

### `GET /api/projects`

Returns the list of known projects for the viewer UI (multi-project selector).

Projects are discovered via a local registry written by active session processes (stdio/shared)
and a durable local catalog updated opportunistically while BranchMind is used.

Additionally, the viewer may perform a **best-effort bounded local filesystem scan** on startup to
seed the durable catalog with any on-disk BranchMind stores (repo-local `.agents/mcp/.branchmind/branchmind_rust.db`),
so older sessions/projects remain selectable even without a recent heartbeat.

The scan is:

- loopback/local only (no network),
- read-only,
- bounded (depth/limits),
- skippable/overridable via env var (power users).

Env override:

- `BRANCHMIND_VIEWER_SCAN_ROOTS=/path/a:/path/b` (colon-separated list of roots to scan).

This enables browsing projects even after the original session exits (read-only), as long as the
project store remains on disk.

`stale=true` means no recent live heartbeat was observed; the project may still be selectable
for read-only viewing.

Response:

```json
{
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "current_project_guard": "string|null",
  "current_label": "string",
  "current_storage_dir": "string",
  "projects": [
    {
      "project_guard": "string",
      "label": "string",
      "storage_dir": "string",
      "workspace_default": "string|null",
      "workspace_recommended": "string|null",
      "updated_at_ms": 0,
      "stale": false,
      "store_present": true,
      "is_temp": false
    }
  ]
}
```

### `GET /api/workspaces`

Returns the list of workspaces in the selected project store. This is used by the viewer UI to
switch between workspaces ("projects" in the human sense) within a repo.

Optional query params:

- `project=<project_guard>`: view another active project (see `/api/projects`).

Response:

```json
{
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "project_guard": "string|null",
  "workspace_default": "string|null",
  "workspace_recommended": "string",
  "workspaces": [
    {
      "workspace": "string",
      "created_at_ms": 0,
      "project_guard": "string|null"
    }
  ]
}
```

### `GET /api/snapshot`

Returns the current **read-only** snapshot of goals/plans/tasks for the default workspace.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the snapshot to another active project (read-only).
- `lens=work|knowledge`: select the snapshot lens (default: `work`).

Response shape (high-level):

```json
{
  "lens": "work|knowledge",
  "workspace": "string",
  "workspace_exists": true,
  "project_guard": {
    "expected": "string|null",
    "stored": "string|null",
    "status": "ok|uninitialized|unknown|not_applicable"
  },
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "focus": {
    "kind": "plan|task|none",
    "id": "string|null",
    "title": "string|null",
    "plan_id": "string|null"
  },
  "primary_plan_id": "string|null",
  "plans_total": 0,
  "tasks_total": 0,
  "plans": [
    {
      "id": "PLAN-###",
      "title": "string",
      "description": "string|null",
      "context": "string|null",
      "status": "string",
      "priority": "string",
      "updated_at_ms": 0,
      "task_counts": { "total": 0, "active": 0, "backlog": 0, "parked": 0, "done": 0 }
    }
  ],
  "plan_checklist": {
    "plan_id": "PLAN-###",
    "current": 0,
    "steps": ["string"]
  },
  "plan_checklists": {
    "PLAN-###": { "plan_id": "PLAN-###", "current": 0, "steps": ["string"] }
  },
  "tasks": [
    {
      "id": "TASK-###",
      "plan_id": "PLAN-###",
      "title": "string",
      "description": "string|null",
      "context": "string|null",
      "status": "string",
      "priority": "string",
      "blocked": false,
      "updated_at_ms": 0,
      "parked_until_ts_ms": 0
    }
  ],
  "truncated": {
    "plans": false,
    "tasks": false
  }
}
```

Notes:

- `workspace_exists=false` means no data is available yet (empty store).
- If `project_guard.status="uninitialized"`, the store predates project-guard enforcement. The
  viewer allows read-only browsing, but the workspace has not been "locked" to a project yet. To
  initialize it, open the workspace via MCP once (any tool call that targets that workspace).
- `plan_checklists` includes checklists for the listed plans (may be empty if none exist).
- `plans_total` / `tasks_total` reflect the full workspace counts even when `plans` / `tasks` lists are truncated.
- `plans[*].task_counts` are computed from the full task table (not from the truncated `tasks` list), so counts remain
  correct even when `truncated.tasks=true`.
- The viewer never writes to the store.
- If a project guard mismatch is detected, the snapshot returns a typed error payload instead.
- If `project=` is invalid or unknown, the snapshot returns a typed error payload.
- Lens `knowledge` is a viewer-only “meaning map” view:
  - `plans[*]` represent **anchors** (ids like `a:viewer`).
  - `tasks[*]` represent **knowledge keys** (ids like `KN:a:viewer:events-sse-live`) and include `card_id` for opening
    the underlying knowledge card via MCP (or future viewer endpoints).
  - `plan_checklist` is always `null` and `plan_checklists` is `{}`.

### `GET /api/search`

Returns a bounded server-side search result set for viewer navigation (Ctrl+K / command palette).

This endpoint exists so navigation can scale **past** `/api/snapshot` truncation without increasing
the snapshot caps. It is still read-only and local-only.

Optional query params:

- `q=<string>`: case-insensitive substring query.
  - Empty/omitted query returns `items=[]`.
- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the search to another active project (read-only).
- `lens=work|knowledge`: select the search lens (default: `work`).
- `limit=<int>`: maximum number of returned items (default: 60; clamped to 1..120).

Response:

```json
{
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "lens": "work|knowledge",
  "workspace": "string",
  "workspace_exists": true,
  "query": "string",
  "limit": 60,
  "items": [
    { "kind": "plan", "id": "PLAN-123", "title": "string", "plan_id": "PLAN-123" },
    { "kind": "task", "id": "TASK-456", "title": "string", "plan_id": "PLAN-123" },
    { "kind": "anchor", "id": "a:viewer", "title": "string", "plan_id": "a:viewer" },
    {
      "kind": "knowledge_key",
      "id": "KN:a:viewer:events-sse-live",
      "title": "events-sse-live",
      "plan_id": "a:viewer",
      "anchor_id": "a:viewer",
      "key": "events-sse-live",
      "card_id": "CARD-KN-..."
    }
  ],
  "has_more": false
}
```

Notes:

- Lens `work` searches plans/tasks by `id` and `title` (best-effort match).
- Lens `knowledge` searches anchors/knowledge keys by `id`/`title`/`key` (best-effort match).
- Ordering is stable (deterministic) and bounded; it is not a full-text index.

### `GET /api/graph/plan/<plan_id>`

Returns a bounded **plan-scoped** subgraph page so the viewer can materialize plan tasks even when
`/api/snapshot` is truncated.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the request to another active project (read-only).
- `lens=work`: only `work` lens is currently supported (default: `work`).
- `limit=<int>`: max returned tasks (default: 200; clamped to 1..600).
- `cursor=<string>`: pagination cursor.
  - For this endpoint, `cursor` is a **TASK id** previously returned as `pagination.next_cursor`.
  - Semantics: return tasks with `id > cursor` (stable `id ASC` ordering).

Response (high-level):

```json
{
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "lens": "work",
  "workspace": "string",
  "workspace_exists": true,
  "plan": { "id": "PLAN-123", "title": "string", "task_counts": { "total": 0 } },
  "tasks_total": 0,
  "tasks": [{ "id": "TASK-456", "plan_id": "PLAN-123", "title": "string" }],
  "pagination": { "cursor": "TASK-0001|null", "limit": 200, "has_more": false, "next_cursor": "TASK-0002|null" }
}
```

Notes:

- This endpoint is deterministic and bounded; it is not an export API.
- The viewer remains read-only; no store mutation occurs.

### `GET /api/graph/cluster/<cluster_id>`

Returns a bounded page of tasks that belong to a **semantic tile cluster**.

Cluster id format:

- `C:<plan_id>:<tileX>:<tileY>` (example: `C:PLAN-123:4:9`).

Tiles are computed from the same deterministic semantic vector that the viewer uses in the graph UI
(tokenization + FNV-1a hashing), then tiled by `tile=0.45`.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the request to another active project (read-only).
- `lens=work`: only `work` lens is currently supported (default: `work`).
- `limit=<int>`: max returned tasks (default: 200; clamped to 1..600).
- `cursor=<string>`: pagination cursor.
  - For this endpoint, `cursor` is the last scanned TASK id (returned as `pagination.next_cursor`).

Notes:

- Cluster paging is **best-effort**: the server must scan tasks in `id ASC` order and apply the same
  deterministic tile function; it does not keep a cluster index in the store yet.
- Because the scan is bounded, `pagination.has_more=true` may also mean “scan budget reached”.

### `GET /api/graph/local/<node_id>`

Returns a bounded “Obsidian local graph” view for a node, even when the node is missing from the
current `/api/snapshot`.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the request to another active project (read-only).
- `lens=work`: only `work` lens is currently supported (default: `work`).
- `hops=1|2`: neighborhood depth (default: 2).
- `limit=<int>`: max returned tasks (default: 200; clamped to 1..600).
- `cursor=<string>`: pagination cursor (forwarded to the underlying plan/cluster pager).

Notes:

- Supported node ids: `PLAN-*` and `TASK-*` (for now).
- For `PLAN-*`, this behaves like `/api/graph/plan/<plan_id>`.
- For `TASK-*` and `hops=2`, the server returns the root task plus a bounded page of tasks from the
  same semantic cluster tile (focus + context).

### `GET /api/events` (SSE)

Returns a **local-only** Server-Sent Events stream of new store events for the selected workspace.

This endpoint is designed to make the viewer feel “live” without aggressive polling. The client
subscribes via `EventSource` and refreshes `/api/snapshot` when meaningful events arrive.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the stream to another active project (read-only).
- `since=<evt_0000000000000000>`: start streaming events strictly after this id.
  - If omitted, the stream starts at “now” (tails the current head; does not replay history).
- `poll_ms=<int>`: server poll interval (default: 250; clamped to 50..2000).
- `keepalive_ms=<int>`: keepalive comment interval (default: 15000; clamped to 5000..60000).
- `max_events=<int>`: max events per connection before the server closes it (default: 400; clamped to 50..2000).
- `max_stream_ms=<int>`: max connection lifetime in ms before the server closes it (default: 120000; clamped to 10000..600000).

Response:

- `Content-Type: text/event-stream`
- Events:
  - `event: ready` (sent immediately; includes an `id:` so EventSource can resume via `Last-Event-ID`)
  - `event: bm_event` (one per new store event)
  - `event: eof` (server budget reached; client should reconnect)

### `GET /api/plan/<PLAN-ID>`

Returns **read-only** details for a single plan, plus its latest local notes/thoughts.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the plan detail to another active project (read-only).
- `trace_cursor=<i64>`: pagination cursor for `trace_tail` (use `next_cursor` from the previous response).
- `notes_cursor=<i64>`: pagination cursor for `notes_tail` (use `next_cursor` from the previous response).

High-level response shape:

```json
{
  "workspace": "string",
  "project_guard": {
    "expected": "string|null",
    "stored": "string|null",
    "status": "ok|uninitialized|unknown|not_applicable"
  },
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "plan": {
    "id": "PLAN-###",
    "title": "string",
    "description": "string|null",
    "context": "string|null",
    "status": "string",
    "priority": "string",
    "updated_at_ms": 0
  },
  "trace_tail": {
    "branch": "string",
    "doc": "string",
    "entries": [
      {
        "seq": 0,
        "ts_ms": 0,
        "kind": "note|event",
        "title": "string|null",
        "format": "string|null",
        "content": "string|null",
        "event_type": "string|null",
        "task_id": "string|null",
        "path": "string|null"
      }
    ],
    "has_more": false,
    "next_cursor": 0
  },
  "notes_tail": { "branch": "string", "doc": "string", "entries": [], "has_more": false, "next_cursor": 0 }
}
```

### `GET /api/task/<TASK-ID>`

Returns **read-only** details for a single task, including its steps and its latest local notes/thoughts.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the task detail to another active project (read-only).
- `trace_cursor=<i64>`: pagination cursor for `trace_tail` (use `next_cursor` from the previous response).
- `notes_cursor=<i64>`: pagination cursor for `notes_tail` (use `next_cursor` from the previous response).

High-level response shape:

```json
{
  "workspace": "string",
  "project_guard": {
    "expected": "string|null",
    "stored": "string|null",
    "status": "ok|uninitialized|unknown|not_applicable"
  },
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "task": {
    "id": "TASK-###",
    "plan_id": "PLAN-###",
    "title": "string",
    "description": "string|null",
    "context": "string|null",
    "status": "string",
    "priority": "string",
    "blocked": false,
    "updated_at_ms": 0,
    "parked_until_ts_ms": 0
  },
  "steps": {
    "items": [
      {
        "path": "s:1.2",
        "title": "string",
        "completed": false,
        "created_at_ms": 0,
        "updated_at_ms": 0,
        "completed_at_ms": 0,
        "criteria_confirmed": false,
        "tests_confirmed": false,
        "security_confirmed": false,
        "perf_confirmed": false,
        "docs_confirmed": false,
        "blocked": false,
        "block_reason": "string|null"
      }
    ],
    "truncated": false
  },
  "trace_tail": { "branch": "string", "doc": "string", "entries": [], "has_more": false, "next_cursor": 0 },
  "notes_tail": { "branch": "string", "doc": "string", "entries": [], "has_more": false, "next_cursor": 0 }
}
```

Budgets / safety notes:

- `steps.items` is capped (currently `<=400`) and may set `steps.truncated=true`.
- `trace_tail.entries` and `notes_tail.entries` are capped (currently `<=64`).
- Entry `content` is truncated (currently `<=20000` chars) to keep responses bounded.

### `GET /api/knowledge/<CARD-ID>`

Returns **read-only** details for a single knowledge card (by `card_id`).

This endpoint exists so the viewer UI can show the actual text behind a `knowledge_key` selection,
without requiring an MCP client.

Optional query params:

- `workspace=<WorkspaceId>`: override workspace for this request (read-only view selection).
- `project=<project_guard>`: switch the request to another active project (read-only).
- `max_chars=<int>`: maximum number of characters returned for `card.text`
  - default: `20000`
  - clamped to `0..200000`

Response (high-level):

```json
{
  "workspace": "string",
  "project_guard": {
    "expected": "string|null",
    "stored": "string|null",
    "status": "ok|uninitialized|unknown|not_applicable"
  },
  "generated_at": "RFC3339",
  "generated_at_ms": 0,
  "card": {
    "id": "CARD-KN-...",
    "type": "knowledge",
    "title": "string|null",
    "text": "string|null",
    "tags": ["string"],
    "status": "string|null",
    "meta_json": "string|null",
    "deleted": false,
    "last_seq": 0,
    "last_ts_ms": 0
  },
  "supports": ["string"],
  "blocks": ["string"],
  "truncated": false,
  "limits": { "max_chars": 20000 }
}
```

Notes:

- The viewer never writes to the store.
- `truncated=true` means the returned `card.text` was cut to `limits.max_chars`.

### `GET /api/settings`

Returns current viewer runtime settings:

```json
{
  "runner_autostart": { "enabled": true, "dry_run": false }
}
```

Optional query params:

- `project=<project_guard>`: view another project. For non-current projects, this endpoint returns
  `enabled=false` (autostart cannot be controlled across sessions).

### `POST /api/settings/runner_autostart`

Updates runner autostart (runtime-only). This is the only intentional viewer side-effect, and it is
explicit user input (tumbler in UI), not a background mutation.

Request:

```json
{ "enabled": true }
```
