#![forbid(unsafe_code)]

/// Minimal root page.
///
/// The viewer UI is now shipped as a separate desktop app (Tauri+Vite+React).
/// The MCP server only exposes the loopback-only viewer **API** under `/api/*`.
pub(crate) const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>BranchMind Viewer API</title>
    <style>
      body {
        font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial,
          "Apple Color Emoji", "Segoe UI Emoji";
        margin: 24px;
        line-height: 1.4;
      }
      code {
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
          "Courier New", monospace;
      }
    </style>
  </head>
  <body>
    <h1>BranchMind Viewer API</h1>
    <p>
      The UI is shipped as a desktop app. Start it from the repo root:
      <code>make run-viewer-tauri</code>
    </p>
    <p>
      This HTTP server exposes read-only endpoints under <code>/api/*</code> (see
      <code>docs/contracts/VIEWER.md</code>).
    </p>
  </body>
</html>
"#;
