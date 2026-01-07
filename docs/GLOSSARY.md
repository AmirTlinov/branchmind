# Glossary

This glossary keeps terms consistent across task execution and reasoning memory.

## Execution domain

- **Plan**: A top-level container that holds a contract and a checklist of steps (high-level runway).
- **Task**: A work item that is executed by completing a tree of steps.
- **Step**: The smallest checkpointable unit of work inside a task.
- **Checkpoint**: A gate that must be confirmed before a step/task can be closed (e.g., criteria/tests/security/perf/docs).
- **Revision**: A monotonic integer used for optimistic concurrency and to prevent lost updates.
- **Focus**: A convenience pointer for targeting, never the source of truth.

## Reasoning domain

- **Artifact**: Explicit content stored intentionally (notes, drafts, diffs, evidence).
- **Event**: An append-only record describing what changed and why (produced by mutations).
- **Branch**: A named pointer to a history head used for what-if exploration.
- **Merge**: An explicit integration step; may produce conflicts.
- **Conflict**: A first-class entity representing incompatible changes that must be resolved explicitly.
- **Graph**: A typed knowledge graph linking hypotheses/questions/tests/evidence/decisions.
- **Trace**: A structured, time-ordered sequence of thinking cards/events for recoverability.
- **Step focus ("room")**: The current step slice used as the default retrieval scope for smart views (typically the first open step of the focused task).
- **Lane**: A noise-isolation “stripe” inside a workspace (`shared` + per-agent lanes) for parallel work without cross-talk.
- **Publish**: An explicit promotion of an artifact from an agent lane into the shared lane (often as a pinned anchor).
- **Cold archive**: A retrieval policy where closed/unpinned/unscoped history is excluded from default views and only returned on explicit request.

## Integration

- **Reasoning reference**: Stable identifiers (branch/docs) that bind a task/plan to its reasoning memory.
- **Single organism**: A guarantee that task mutations and reasoning events stay consistent without manual sync.
