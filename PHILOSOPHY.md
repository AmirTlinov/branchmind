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

## 4) Branching is how uncertainty is handled

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

## 6) Correctness gates protect progress

The system is designed to prevent “false done”:

- checkpoints gate completion,
- evidence is attached intentionally,
- revisions prevent lost updates,
- strict targeting prevents silent mis-targeting.

This is not bureaucratic: it is a safety harness for long, high-stakes work.

## 7) MCP-only, IDE-ready

The server is a pure backend: no GUI/TUI. It exists to be embedded into an AI-first IDE and used by agents programmatically.

