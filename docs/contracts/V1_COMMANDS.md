# Contracts — v1 Command Registry (SSOT)

This document is the **stable index of v1 commands** (`cmd`). It is the single source of truth
for public-facing operations, with schema discovery via `system` → `schema.get`.

## Command index {#cmd-index}

Advanced/legacy commands may share this anchor. Use `system` → `schema.get(cmd)` for exact
arguments, examples, and budget defaults.

---

## system.schema.get

Return the schema bundle for a command (`args_schema`, `example_minimal_args`,
`example_valid_call`, `doc_ref`).

Notes:
- Runtime is **fail-open**: the server always returns the schema bundle even if local docs are unavailable.
  Docs drift is enforced by CI/test guards (not in the agent UX loop).

## system.cmd.list

List all registered `cmd` names (SSOT registry).

## system.ops.summary

Return a small, low-noise summary of the v1 UX surface:

- tool surface count + names (must be 10),
- golden ops count (as advertised in `tools/list`),
- cmd registry count (and cmd-by-domain counts),
- unplugged ops (if any) to detect “advertised but not dispatchable” drift.

## system.migration.lookup

Map old tool name → `cmd` and return a minimal call example.

## system.storage

Low-level storage introspection (legacy `storage`). Intended for debugging / internal ops.

## system.init

Initialize a workspace (legacy `init`).

## system.help

Help / quick reference (legacy `help`).

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
- v1 UX defaults to the workspace knowledge base scope (`kb/main`, docs: `kb-graph`, `kb-trace`).

## think.knowledge.query

List knowledge cards (bounded, step-aware).

Notes:
- v1 UX defaults to the workspace knowledge base scope (`kb/main`, docs: `kb-graph`).

## think.knowledge.recall

Fast knowledge recall by anchor (bounded, recency-first).

Notes:
- Intended for “I’m touching component X → pull relevant knowledge” UX.
- Uses the storage knowledge index (not a full tag scan).

## think.knowledge.lint

Lint knowledge key hygiene (precision-first) and propose consolidation actions.

This command is intended to keep `think.knowledge.recall` cheap and high-signal across long-lived,
research-heavy workspaces by helping agents:

- detect high-confidence duplicate keys (same content under different keys),
- spot potentially too-generic / overloaded keys via objective metrics (fanout / variants),
- open the exact cards involved so consolidation is cheap.

### Inputs (selected)

- `limit` (int): max number of knowledge key index rows to scan (budget-capped).
- `anchor` (string | string[]): optional anchor(s) to restrict lint to a subset (same format as
  `think.knowledge.recall`).
- `include_drafts` (bool): include draft-lane knowledge (default `false`).
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
