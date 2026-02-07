# Philosophy — “Execution and Reasoning as One Workstream”

High-quality engineering is not just doing steps. It is:

- keeping intent stable while details evolve,
- making decisions reversible when uncertainty is high,
- preserving evidence and context across time.

This project treats **task execution** and **reasoning memory** as two synchronized views of the same workstream.

## 1) One workstream, two views

- **Execution view** answers: *What are we doing, what is done, what is blocked, what must be verified?*
- **Reasoning view** answers: *Why did we choose this, what did we try, what did we learn, what evidence supports it?*

Neither view is optional for long, complex work. The system exists to make both cheap and reliable.

## 2) Artifacts over hidden thought

The system stores **explicit artifacts** that an agent chooses to record:

- notes, decisions, hypotheses, tests, evidence links,
- task plans and checkpoints,
- diffs and merges of drafts.

It must not store or infer private chain-of-thought. Quality comes from explicitness and auditability, not from hidden reasoning.

## 3) Determinism is a UX feature

An agent can only trust a tool that behaves predictably:

- same input → same output,
- schema-stable responses,
- explicit budgets and explicit truncation,
- typed errors with concrete recovery paths.

“Magic convenience” is allowed only when it is safe, explainable, and recoverable.

## 4) Human situational awareness is optional, not a dependency

The core remains MCP-only and automation-first. Human situational awareness must be achieved via
MCP outputs and explicit exports; it must never become a dependency of core execution or
reasoning behavior.

## 5) Branching is how uncertainty is handled

When the right path is unclear:

- branch into alternatives (“hypotheses”),
- make small commits/events as evidence accumulates,
- compare by diffs,
- merge improvements deliberately.

Selection is not enough; **integration** is the core move.

## 5) Low noise, high signal

Default outputs must fit within a small context budget:

- summaries before full data,
- deltas before snapshots,
- diffs before re-reading.

Large payloads are always opt-in and always bounded.

Tool count is also noise. A tool surface that is too wide forces agents to spend tokens on command selection and
repeating boilerplate. The ideal is a **small daily-driver subset** plus a deeper expert surface when needed.
Where possible, prefer macros and templates over manual step-by-step ceremony: fewer tokens spent on syntax means
more tokens spent on actual reasoning and verification.

To keep outputs cognitively cheap, the system also treats **response boilerplate** as a UX budget. Repeating large
structured payloads, verbose warnings, and long “how to use this” explanations can easily dominate the agent context.

Flagship rule: **semantics belong in a legend, not in every response.**

- Portal tools are **context-first** and render a compact tagged line protocol (BM-L1).
- The meaning of tags is defined once via an explicit reference surface (contracts + architecture docs),
  and discoverable via a dedicated `help` tool (so daily outputs stay quiet).
- Full structured payloads should be provided via explicit “full view” tools (and by using the full toolset), not by
  teaching agents to switch portal formats.

## 5.1) Instant handoff is a product feature

A good system lets one agent “run the lap” and another agent pick up the baton without confusion.

- A newcomer agent should be able to load **one bounded snapshot** and immediately know:
  - what is the goal and constraints,
  - what was done vs. what is only planned,
  - what is blocked and why,
  - what must be verified next,
  - what decisions were made and what evidence supports them.
- This is why we treat summaries, reasoning traces, and diffs as first-class outputs: they are the handoff capsule.

## 5.2) Proof-first is how we prevent “false progress”

Agents can produce confident text that is not backed by reality. This system is designed to make reality cheap to attach.

- A **proof** is an explicit artifact/check/link that can be inspected later (CI run, command output, diff, external URI).
- Proofs are attached to **checkpoints** (`tests`, `security`, `perf`, `docs`) so “gates” are not abstract booleans; they are gates **with receipts**.
- Proofs should be **copy/paste-ready** for agents: prefer short receipt lines like `CMD: ...` (what you ran) and `LINK: ...` (where to verify it).
- Proof input should be **syntax-minimal**: if an agent pastes a raw command or a URL, the system should auto-normalize it into the receipt format (and softly warn when receipts are incomplete).
- Proof enforcement is intentionally **hybrid**:
  - most steps are warning-first (to avoid bureaucracy),
  - critical steps in principal workflows are require-first (to prevent shipping without evidence).

The result should feel like a safety harness: it does not slow you down when everything is fine, but it catches you when you are about to declare “done” without receipts.

## 6) Correctness gates protect progress

The system is designed to prevent “false done”:

- checkpoints gate completion,
- evidence is attached intentionally,
- revisions prevent lost updates,
- strict targeting prevents silent mis-targeting.

This is not bureaucratic: it is a safety harness for long, high-stakes work.

## 7) MCP-only, IDE-ready

The server is a pure backend: no GUI/TUI. It exists to be embedded into an AI-first IDE and used by agents programmatically.

## 8) Agent-first engineering

This repository is built to be maintained by **agents** as a first-class workflow, not as an afterthought.

That changes what “good code” means:

- **Small surfaces beat cleverness.** Prefer simple, explicit APIs that stay stable as features grow.
- **Local reasoning beats global context.** Keep modules small enough that an agent can load the whole file and make safe edits.
- **Contracts are the product.** Code is an implementation of the contract; tests are the enforcement mechanism.

## 9) No monoliths: build with seams

Monoliths fail agents the same way they fail humans: they hide dependencies and make edits risky.

We build with seams:

- Split tool implementations by family and by tool.
- Prefer “one responsibility per module” and “one type of persistence per adapter function”.
- Use request structs (single input object) for evolving APIs.
- Keep error mapping and budget enforcement near the boundary (MCP adapter), not in the domain core.

## 10) Fix ambiguity with explicit decisions

Agents are fast, but strategic ambiguity creates forks, conflicts, and wasted tokens. This project intentionally fixes a few
high-leverage decisions so day-to-day work stays deterministic:

- Tool surface stays split into two families: `tasks_*` (execution) and **unprefixed reasoning/memory tools**.
  The MCP server name is the namespace, so agents call them as `branchmind.status`, `branchmind.macro_branch_note`, etc.
- Persistence stays a **single embedded transactional store** so “execution + reasoning” cannot drift.
- Dependency policy is “minimal audited deps” in adapters; the domain core stays std-only.

If any of these change, treat it as a contract + architecture change, not a casual refactor.

## 11) Iteration beats linearity

Most agents default to a straight line: one idea → one plan → one implementation. Humans do not.
Good engineering is closer to a research loop:

- draft a hypothesis,
- test it,
- record evidence,
- branch when uncertainty is high,
- merge back only what survived reality.

Branching + diffing + merging are not “extra features”. They are the mechanism that makes agents think
more like careful engineers and less like single-pass text generators.
