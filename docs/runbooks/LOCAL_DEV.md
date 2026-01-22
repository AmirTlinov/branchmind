# Local development

## Golden path

```bash
make check
make run-mcp
```

## Storage

- Default store directory (auto mode): `.branchmind_rust/`
- Override: `--storage-dir <path>`

## Workspace safety

- Default workspace (auto mode): derived from repo root directory name.
- Workspace lock (auto mode): enabled by default to prevent accidental cross-project writes.

## Toolsets

- `daily`: minimal portal surface for everyday agent work.
- `full`: full parity surface.
- `core`: ultra-minimal tool surface.

Override with `--toolset` or `BRANCHMIND_TOOLSET`.
