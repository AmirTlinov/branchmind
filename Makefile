.PHONY: help fmt fmt-check clippy test check run-mcp viewer-install viewer-typecheck viewer-build viewer-tauri-dev

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
		"" \
		"Viewer (Tauri, optional):" \
		"  make viewer-install   Install npm deps (apps/viewer-tauri/)" \
		"  make viewer-typecheck Typecheck the viewer" \
		"  make viewer-build     Build the viewer frontend (Vite)" \
		"  make viewer-tauri-dev Run the desktop viewer (tauri dev)" \
		""

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

viewer-install:
	cd apps/viewer-tauri && npm install

viewer-typecheck:
	cd apps/viewer-tauri && npm run typecheck

viewer-build:
	cd apps/viewer-tauri && npm run build

viewer-tauri-dev:
	cd apps/viewer-tauri && npm run tauri:dev
