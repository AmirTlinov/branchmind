.PHONY: help fmt fmt-check clippy test check run-mcp run-viewer-tauri viewer-build

CARGO ?= cargo

help:
	@printf "%s\n" \
		"Targets:" \
		"  make check      Run fmt-check + clippy + tests" \
		"  make fmt        Apply rustfmt" \
		"  make fmt-check  Verify formatting" \
		"  make clippy     Run clippy (deny warnings)" \
		"  make test       Run workspace tests" \
		"  make run-mcp    Run MCP server (DX defaults)" \
		"  make run-viewer-tauri  Run viewer desktop shell (Tauri, optional)" \
		"  make viewer-build      Build viewer-app + copy assets (optional)"

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --workspace

check: fmt-check clippy test

# Golden path: zero-arg run enables DX defaults.
run-mcp:
	$(CARGO) run -p bm_mcp

run-viewer-tauri:
	$(CARGO) run --manifest-path viewer-tauri/src-tauri/Cargo.toml

viewer-build:
	cd viewer-app && npm run build
	bash viewer-app/scripts/copy-assets.sh
	$(CARGO) check -p bm_mcp
