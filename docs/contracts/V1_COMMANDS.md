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

## system.cmd.list

List all registered `cmd` names (SSOT registry).

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

## jobs.list

List jobs (legacy `tasks_jobs_list`).

## jobs.radar

Low-noise job radar (legacy `tasks_jobs_radar`).

## jobs.open

Open a job record (legacy `tasks_jobs_open`).

## jobs.runner.heartbeat

Runner heartbeat + capabilities (legacy `tasks_runner_heartbeat`).

---

## think.knowledge.upsert

Upsert a knowledge card with fingerprint-based de-duplication.

## think.knowledge.query

List knowledge cards (bounded, step-aware).

## think.knowledge.lint

Lint knowledge density and propose consolidation actions.

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

## vcs.branch.merge

Merge a branch (graph merge fallback).

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
