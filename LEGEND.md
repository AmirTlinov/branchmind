[LEGEND]
DOC_FORMAT = The canonical doc shape: a `[LEGEND]` block then a `[CONTENT]` block.
LEGEND_BLOCK = The `[LEGEND]` block containing definitions.
CONTENT_BLOCK = The `[CONTENT]` block containing the document body.
TOKEN = A named meaning reused across docs.
GLOBAL_TOKEN = A token defined in `LEGEND.md`; available repo-wide.
LOCAL_TOKEN = A token defined in a specific doc; scoped to that doc.
TOKEN_REF = A token reference in content: `[[TOKEN]]` (global, Obsidian-friendly) or `[TOKEN]` (local/legacy).
TOKEN_PAGE = A generated token page under `docs/tokens/TOKEN.md` (built by `./tools/context`).
GRAPH_HUB = Generated navigation hubs (built by `./tools/context`): `docs/GRAPH.md` (token/contract hub) and `docs/GRAPH_DOCS.md` (doc-to-doc hub).
NO_SHADOWING = Rule: a doc must not redefine a global token locally.
GATE = A deterministic checker that fails closed on drift.
DOCTOR = A diagnostic checker for environment + repo foundation.
CONTRACT = A versioned interface spec with examples.
CHANGE_PROTOCOL = The sequence: contracts → implementation → tests → docs.

[CONTENT]
This file is the global vocabulary for the repo.

Use it when:
- A meaning repeats across multiple documents.
- You want agents to reuse the same mental model without re-parsing prose.

Avoid it when:
- The concept is unique to one doc (keep it local).
