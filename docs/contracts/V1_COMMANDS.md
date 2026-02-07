# Contracts — v1 Command Registry (SSOT)

This document is the **stable index of v1 commands** (`cmd`). It is the single source of truth
for public-facing operations, with schema discovery via `system` → `schema.get`.

## Command index {#cmd-index}

Advanced/internal commands may share this anchor. Use `system` → `schema.get(cmd)` for exact
arguments, examples, and budget defaults.

---

## system.schema.get

Return the schema bundle for a command (`args_schema`, `example_minimal_args`,
`example_valid_call`, `doc_ref`).

Notes:
- Runtime is **fail-open**: the server always returns the schema bundle even if local docs are unavailable.
  Docs drift is enforced by CI/test guards (not in the agent UX loop).

## system.cmd.list

List registered `cmd` names (SSOT registry).

UX notes:

- Default behavior is **kernel-first**: returns only the small curated “kernel surface” (golden ops +
  a handful of workflow macros), and only those available in the current toolset.
- Use `include_hidden=true` to list the **full registry** (may include advanced/internal commands).

## system.daemon.restart

Force a **shared-mode daemon restart** (best-effort).

Purpose:
- After a local rebuild, a long-lived shared proxy may still be connected to an older daemon process.
- This command provides an explicit **one-command escape hatch** for agents/users.

Semantics (shared mode):
- The shared proxy requests daemon shutdown (best-effort),
- unlinks the daemon socket path,
- drops its daemon connection.
- The **next forwarded request** will spawn a fresh daemon via the normal `connect_or_spawn` path.

Auto-heal note (shared mode):
- The shared proxy also performs **stale-daemon avoidance**: when the compat fingerprint matches
  but the daemon’s `build_time_ms` is older than the proxy binary, the proxy treats the daemon as
  incompatible and restarts it automatically (no manual action required).
- `system.daemon.restart` remains as an explicit escape hatch.

Semantics (non-shared mode):
- Returns a typed `NOT_SUPPORTED` error with an actionable recovery hint.

Inputs: none.

## system.ops.summary

Return a small, low-noise summary of the v1 UX surface:

- tool surface count + names (must be 10),
- golden ops count (as advertised in `tools/list`),
- cmd registry count (and cmd-by-domain counts) **for the active toolset** (Core/Daily/Full),
- kernel cmd count (and kernel cmd-by-domain counts) for cognitive-cheap onboarding,
- unplugged ops (if any) to detect “advertised but not dispatchable” drift.

## system.storage

Low-level storage introspection (internal). Intended for debugging / internal ops.

## system.init

Initialize a workspace (internal).

## system.help

Help / quick reference.

## system.tutorial

Guided onboarding (actions-first).

Inputs (selected):

- `limit` (int, optional): max onboarding steps returned (default 3).
- `max_chars` (int, optional): max chars for the tutorial summary text.

Output (selected):

- `{ title, summary, steps:[...], truncated }`

Notes:

- Steps follow the golden path: `status → tasks.macro.start → tasks.snapshot`.
- `actions[]` includes executable calls for each returned step (bounded by `limit`).
- If `workspace` is not set, actions rely on the default workspace (or call `workspace.use` first).
- `truncated=true` when `limit` or `max_chars` cuts the tutorial output.

## system.skill

Skill discovery / info (legacy `skill`).

## system.diagnostics

Diagnostics snapshot (legacy `diagnostics`). Intended for debugging / internal ops.

---

## workspace.use

Switch the active workspace for the session.

## workspace.reset

Clear the workspace override and return to the default/auto workspace.

---

## tasks.plan.create

Create a plan or task (legacy `tasks_create`).

## tasks.plan.decompose

Add steps to a task/plan (legacy `tasks_decompose`).

## tasks.evidence.capture

Attach proof artifacts/checks to a step or task (legacy `tasks_evidence_capture`).

## tasks.step.close

Confirm checkpoints and close a step (legacy `tasks_close_step`).

## tasks.execute.next

Return NextEngine actions for the current focus.

---

## jobs.create

Create a delegation job (legacy `tasks_jobs_create`).

Notes:
- If the runner is offline and autostart is enabled, the server may auto-start `bm_runner`.
- The response may include `runner_autostart` and (when needed) `runner_bootstrap` (copy/paste command).

## jobs.list

List jobs (legacy `tasks_jobs_list`).

## jobs.radar

Low-noise job radar (legacy `tasks_jobs_radar`).

## jobs.open

Open a job record (legacy `tasks_jobs_open`).

## jobs.proof.attach

Attach proof receipts from a job to a task/step (legacy `tasks_jobs_proof_attach`).

Notes:
- Input includes `{ job, task?, step_id?|path?, checkpoint?, artifact_ref?, max_file_bytes? }`.
- The server resolves stable refs from the job (summary/refs + `artifact_ref`) and records evidence.
- Attachments are emitted as `LINK: file://...` (with `sha256` when available) when possible.
- `max_file_bytes` bounds sha256 hashing (default: 64 MiB per file, best-effort).

## jobs.cancel

Cancel a **queued** job (QUEUED → CANCELED).

Notes:
- This is **queued-only**. If a job is RUNNING, the tool returns `error.code="CONFLICT"` and provides
  actions to (a) `jobs.open` and (b) `jobs.complete status=CANCELED` with prefilled `runner_id` + `claim_revision`.
- Use `system` → `schema.get(cmd)` for the exact arguments.

## jobs.wait

Wait for a job to reach a terminal status (DONE/FAILED/CANCELED).

Notes:
- Transport-safe by design: the server clamps the blocking portion of the call (see
  `result.effective_timeout_ms`) so `jobs.wait` stays safe under typical MCP deadlines.
- `timeout_ms` is a **desired** wait budget, but may be clamped per call. To wait longer, call the
  command again (actions include a ready-to-run continuation call).
- On timeout/clamp, the tool returns `success=true` with `result.done=false` (not an error).
- Output includes: `waited_ms`, `requested_timeout_ms`, `effective_timeout_ms`, `remaining_ms`, and
  the current `job` row snapshot.
- Optional `mode`:
  - `mode="default"` (default): structured JSON + `actions[]`.
  - `mode="watch"`: returns **1–2 lines** (line protocol) intended for agent polling loops:
    a compact status line with **stop-condition hints**, plus (when `done=false`) a copy/paste
    continuation command that preserves `mode="watch"`.
- Use `system` → `schema.get(cmd)` for the exact arguments.

## jobs.runner.start

Explicitly start the first-party `bm_runner` for the workspace (best-effort).

Notes:
- This is allowed because `bm_mcp` may auto-start the first-party runner (see `DELEGATION.md`).
- Runtime is **fail-open**: on failure, the server returns `runner_bootstrap` (copy/paste command)
  and emits `warnings[]` with `code="RUNNER_START_FAILED"`.

## jobs.runner.heartbeat

Runner heartbeat + capabilities (legacy `tasks_runner_heartbeat`).

---

## think.knowledge.upsert

Upsert a knowledge card.

Notes:
- When `args.key` is provided (together with `args.anchor`), the command uses a **stable identity**
  `(anchor,key)` with **versioned card_ids**. Editing the text produces a new `card_id` and updates
  the knowledge index so `think.knowledge.recall` and future `think.knowledge.upsert` calls resolve
  to the latest version (history stays in the graph).
- Visibility: knowledge defaults to `v:draft` unless explicitly tagged (`v:canon`) or later promoted via publish.
- v1 UX defaults to the workspace knowledge base scope (`kb/main`, docs: `kb-graph`, `kb-trace`).
- `key_mode`:
  - `explicit` (default): requires `anchor` + `key` when a stable identity is desired.
  - `auto`: derives a deterministic key from the card title/text when `key` is omitted.
- `lint_mode`:
  - `manual` (default): no lint side-effects.
  - `auto`: emits bounded, low-noise warnings for key hygiene (no blocking).

## think.knowledge.key.suggest

Suggest a stable knowledge key for a given anchor/title.

Notes:
- Returns `{ suggested_key, key_tag, collisions[] }`.
- `collisions[]` lists existing `(anchor,key)` hits to prevent noisy key reuse.

## think.knowledge.query

List knowledge cards (bounded, step-aware).

Notes:
- v1 UX defaults to the workspace knowledge base scope (`kb/main`, docs: `kb-graph`).
- Convenience filter: `args.key=<key>` limits results to a single knowledge key (useful for reviewing
  lint findings / consolidation candidates).
- When `args.key` is present and `include_history=false`, the command uses the storage knowledge key
  index to resolve the *latest* card per `(anchor,key)` (no historical duplicates).
- Defaults are product-UX oriented:
  - `include_drafts=true` (management view; show what’s in the KB, not only what’s published)
  - `include_history=false` (latest-only; no duplicate historical versions unless explicitly requested)
- Use `think.knowledge.recall` for fast “what do we know about X?” recall; use `include_history=true`
  here when you need audit/history across versions.

## think.knowledge.recall

Fast knowledge recall by anchor (bounded, recency-first).

Notes:
- Intended for “I’m touching component X → pull relevant knowledge” UX.
- Anchor-first and lightweight: uses the knowledge key index + graph fetch (not a full tag scan).
- Defaults to `include_drafts=false` (canon-first); for draft-heavy audits use `think.knowledge.query`.

## think.knowledge.lint

Lint knowledge key hygiene (precision-first) and propose consolidation actions.

This command is intended to keep `think.knowledge.recall` cheap and high-signal across long-lived,
research-heavy workspaces by helping agents:

- detect high-confidence duplicate keys (same content under different keys),
- detect same-key duplicate content across anchors (same `(key,content)` in multiple anchors),
- spot potentially too-generic / overloaded keys via objective metrics (fanout / variants),
- open the exact cards involved so consolidation is cheap.

## think.note.promote

Promote an existing `notes@seq` entry into a knowledge card (draft by default).

Notes:
- Input: `{ note_ref, anchor?, key?, title?, key_mode? }`
- Uses the note content as card text; visibility defaults to `v:draft` unless overridden.

### Inputs (selected)

- `limit` (int): max number of knowledge key index rows to scan (budget-capped).
- `anchor` (string | string[]): optional anchor(s) to restrict lint to a subset (same format as
  `think.knowledge.recall`).
- `include_drafts` (bool): include draft-lane knowledge (default `true`).
- `max_chars` (int): output budget knob (injected/clamped by budget profile if omitted).

### Output (selected)

- `result.stats`:
  - `keys_scanned`, `has_more` (from key index pagination)
  - `anchors`, `keys`, `cards_resolved`
  - `issues_total` (before truncation)
- `result.issues[]`: findings objects:
  - `severity`: `warning|info`
  - `code`: stable issue code
  - `message`: human summary
  - `evidence`: structured proof (anchor ids, key slugs, card ids)

### Issue codes (precision-first)

The linter is intentionally conservative: it only emits `warning` when there is strong evidence.

- `KNOWLEDGE_DUPLICATE_CONTENT_SAME_ANCHOR` (`warning`):
  two or more distinct keys under the same anchor resolve to identical normalized content.
- `KNOWLEDGE_DUPLICATE_CONTENT_SAME_KEY_ACROSS_ANCHORS` (`info`):
  the same key is present across multiple anchors with identical content (often a candidate for
  shared/canonical knowledge).
- `KNOWLEDGE_DUPLICATE_CONTENT_ACROSS_ANCHORS_MULTIPLE_KEYS` (`info`):
  identical normalized content appears across multiple anchors under multiple distinct keys (often
  a sign of key drift / duplicated knowledge that can be consolidated).
- `KNOWLEDGE_KEY_OVERLOADED_ACROSS_ANCHORS` (`info`):
  the same key is present across multiple anchors with multiple distinct content variants (potentially
  too-generic / bucketed key).
- `KNOWLEDGE_KEY_OVERLOADED_OUTLIERS` (`info`):
  a special-case of overloaded keys where one content variant dominates and other variants look like
  outliers (the linter includes deterministic evidence to make consolidation cheap).

### Actions (v1 UX)

On success, `actions[]` may include deterministic “open helpers” such as:

- `graph.query` for the exact `ids=[...]` involved in a duplicate set.
- `graph.query` for the exact `ids=[...]` involved in a cross-anchor duplicate-content group.
- `think.knowledge.query` with `args.key=<key>` to review a reused/overloaded key across anchors.
- `graph.query` for the outlier card ids when a key has a dominant variant (`KNOWLEDGE_KEY_OVERLOADED_OUTLIERS`).

## think.reasoning.seed

Seed a reasoning frame/hypothesis template (legacy `think_template`).

## think.reasoning.pipeline

Run the reasoning pipeline (legacy `think_pipeline`).

## think.idea.branch.create

Create an idea branch + capsule card (legacy `macro_branch_note`).

## think.idea.branch.merge

Merge an idea branch + graph state (v1 custom).

---

## graph.apply

Apply graph operations (legacy `graph_apply`).

## graph.query

Query graph view (legacy `graph_query`).

## graph.merge

Merge graph changes (legacy `graph_merge`).

Notes:

- **Resolved conflicts do not re-surface.** Once a conflict is resolved (`status="resolved"`), subsequent
  `graph.merge` calls treat it as handled even if the underlying divergence still exists (e.g. `use_into`),
  preventing “infinite conflict loops”.
- Result counters:
  - `conflicts_detected`: diverged candidates that produced an **open/preview** conflict in the response.
  - `conflicts_created`: new conflict rows inserted into storage (can be `0` in `dry_run=true`, or when
    conflicts already exist).

---

## vcs.branch.create

Create a branch (legacy `branch_create`).

---

## docs.list

List docs for a branch/ref (legacy `docs_list`).

## docs.show

Show document entries (legacy `show`).

## docs.diff

Diff a document between branches (legacy `diff`).

## docs.merge

Merge document entries between branches (legacy `merge`).

## docs.transcripts.search

Search transcripts under `root_dir` for `query` and return bounded matches + openable refs.

## docs.transcripts.digest

Scan transcripts under `root_dir` and return a bounded digest (0..`max_items`) per session.

**Actions on success (v1 UX):**

- May include an `docs` action calling `docs.transcripts.open` for the newest digest item.
- When scan budgets are too tight and no digest items are found, returns warning
  `TRANSCRIPTS_SCAN_TRUNCATED` / `TRANSCRIPTS_MAX_FILES_REACHED` and includes retry actions:
  - retry with a larger scan budget (`max_files`, `max_bytes_total`)
  - optional fallback: retry with `mode="last"` for faster orientation

## docs.transcripts.open

Open a bounded window around a transcript ref (`path` + `line` or `byte`) and return message entries.

**Actions on success (v1 UX):**

- Includes capture actions into reasoning (copy/paste-ready):
  - `think op=call cmd=think.idea.branch.create` (personal lane)
  - `think op=call cmd=think.idea.branch.create` with `agent_id=null` (shared lane)
- The capture action's `args.meta.step` is populated when the workspace focus is a TASK with
  an open step (step-aware grafting).

## docs.export

Export artifacts (legacy `export`). Use `system op=schema.get` for the exact args schema.
