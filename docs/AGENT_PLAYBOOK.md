[LEGEND]
GOLDEN_PATH = The recommended sequence that keeps work safe and deterministic.

[CONTENT]
Golden path ([GOLDEN_PATH]):
1) Read [[MAP.md]].
2) Change contracts first (docs/contracts).
3) Implement.
4) Add/adjust tests.
5) Run `./tools/gate` until green.
6) Refresh the machine + graph index: `./tools/context`:
   - `ai/context_index.json`
   - `docs/GRAPH.md`
   - `docs/GRAPH_DOCS.md`
   - `docs/tokens/*` (global token pages + index)
