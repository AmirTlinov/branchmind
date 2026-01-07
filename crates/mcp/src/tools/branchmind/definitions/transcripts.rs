#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn transcripts_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "transcripts_search",
            "description": "Search across Codex session transcript files under a directory (read-only, bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "root_dir": { "type": "string", "description": "Root directory containing Codex transcript files (recursively). Defaults to CODEX_HOME/sessions or ~/.codex/sessions." },
                    "query": { "type": "string" },
                    "cwd_prefix": { "type": "string", "description": "Optional filter by session project hints prefix (defaults to server project root)." },
                    "role": { "type": "string", "description": "Optional filter by message role (e.g., user|assistant|system)." },
                    "dedupe": { "type": "boolean", "description": "De-duplicate repeated hits across files (default true)." },
                    "max_files": { "type": "integer" },
                    "max_bytes_total": { "type": "integer" },
                    "hits_limit": { "type": "integer" },
                    "context_chars": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "query"]
            }
        }),
        json!({
            "name": "transcripts_digest",
            "description": "Project-scoped digest of recent transcript summary messages (read-only, bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "root_dir": { "type": "string", "description": "Defaults to CODEX_HOME/sessions or ~/.codex/sessions." },
                    "cwd_prefix": { "type": "string", "description": "Optional filter by session project hints prefix (defaults to server project root)." },
                    "mode": { "type": "string", "description": "Selection mode: summary | last" },
                    "max_files": { "type": "integer" },
                    "max_bytes_total": { "type": "integer" },
                    "max_items": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "transcripts_open",
            "description": "Open a bounded window of a transcript file by (path,line) or (path,byte) reference (read-only, bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "root_dir": { "type": "string", "description": "Defaults to CODEX_HOME/sessions or ~/.codex/sessions." },
                    "ref": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "line": { "type": "integer", "description": "1-based JSONL line number (optional if byte is provided)." },
                            "byte": { "type": "integer", "description": "0-based byte offset of the JSONL line start (preferred for huge files)." }
                        },
                        "required": ["path"],
                        "oneOf": [
                            { "required": ["path", "line"] },
                            { "required": ["path", "byte"] }
                        ]
                    },
                    "before_lines": { "type": "integer" },
                    "after_lines": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "ref"]
            }
        }),
    ]
}
