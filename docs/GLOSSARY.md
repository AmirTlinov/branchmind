# Glossary

- **Workspace**: Top-level namespace for all reasoning state.
- **Branch**: Named thought lane with its own head commit.
- **Commit**: Immutable thought entry (`message` + `body`) in a branch history.
- **Head commit**: Latest commit id pointed to by a branch.
- **Parent commit**: Previous commit in the same branch history chain.
- **Merge**: Explicit synthesis of a source branch into a target branch.
- **Merge record**: Durable metadata about a merge action and synthesis commit.
- **Synthesis commit**: Commit created on target branch during merge.
- **Fail-closed**: Unknown/invalid input is rejected explicitly; never ignored.
- **V3 surface**: Active MCP contract with exactly three tools: `branch`, `think`, `merge`.
