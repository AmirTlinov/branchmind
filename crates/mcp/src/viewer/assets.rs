#![forbid(unsafe_code)]

/// Viewer UI shell (single-file HTML with inlined JS/CSS).
///
/// Source of truth: `viewer-tauri/` (Vite+React). This file is copied from
/// `viewer-tauri/dist/index.html` into `crates/mcp/src/viewer/assets/index.html`
/// so `bm_mcp` can serve the viewer without requiring Node tooling at runtime.
pub(crate) const INDEX_HTML: &str = include_str!("assets/index.html");
