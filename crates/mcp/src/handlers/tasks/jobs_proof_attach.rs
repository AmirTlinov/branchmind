#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use sha2::Digest as _;
use std::fmt::Write as _;
use std::io::Read as _;

const DEFAULT_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB

fn sha256_file_hex(path: &std::path::Path) -> Result<String, std::io::Error> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = sha2::Sha256::new();

    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(&mut out, "{:02x}", b);
    }
    Ok(out)
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_proof_attach(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let job_id = match require_string(args_obj, "job") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let task = match optional_string(args_obj, "task") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let artifact_ref = match optional_string(args_obj, "artifact_ref") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let max_refs = match optional_usize(args_obj, "max_refs") {
            Ok(v) => v.unwrap_or(32).clamp(1, 64),
            Err(resp) => return resp,
        };
        let max_file_bytes = match optional_usize(args_obj, "max_file_bytes") {
            Ok(v) => v
                .unwrap_or(DEFAULT_MAX_FILE_BYTES as usize)
                .clamp(1, (512 * 1024 * 1024) as usize) as u64,
            Err(resp) => return resp,
        };

        let open = match self.store.job_open(
            &workspace,
            bm_storage::JobOpenRequest {
                id: job_id.clone(),
                include_prompt: false,
                include_events: true,
                include_meta: false,
                max_events: 50,
                before_seq: None,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown job id"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut refs = Vec::<String>::new();
        for event in &open.events {
            for r in event.refs.iter() {
                let trimmed = r.trim();
                if trimmed.is_empty() {
                    continue;
                }
                refs.push(trimmed.to_string());
            }
        }
        if let Some(artifact) = artifact_ref.as_deref() {
            let trimmed = artifact.trim();
            if !trimmed.is_empty() {
                refs.push(trimmed.to_string());
            }
        }

        let summary_text = open
            .job
            .summary
            .as_deref()
            .or_else(|| open.events.first().map(|e| e.message.as_str()))
            .unwrap_or("");
        let refs = crate::salvage_job_completion_refs(summary_text, &job_id, &refs);
        let mut checks = Vec::<String>::new();
        let mut attachments = Vec::<String>::new();
        let mut files = Vec::<Value>::new();

        for r in refs.into_iter().take(max_refs) {
            let trimmed = r.trim();
            if trimmed.is_empty() {
                continue;
            }
            let upper = trimmed.to_ascii_uppercase();
            if upper.starts_with("CMD:") || upper.starts_with("LINK:") {
                if let Some(coerced) = crate::coerce_proof_check_line(trimmed) {
                    checks.push(coerced);
                }
                continue;
            }
            if crate::looks_like_bare_url(trimmed) {
                checks.push(format!("LINK: {trimmed}"));
                continue;
            }
            // Treat absolute file paths as file:// links when possible.
            if trimmed.starts_with('/')
                && let Ok(path) = std::fs::canonicalize(trimmed)
            {
                let uri = format!("file://{}", path.to_string_lossy());
                let link = format!("LINK: {uri}");
                checks.push(link.clone());

                // Best-effort sha256: bounded and deterministic.
                let entry = match std::fs::metadata(&path) {
                    Ok(meta) => {
                        let bytes = meta.len();
                        if bytes <= max_file_bytes {
                            match sha256_file_hex(&path) {
                                Ok(sha256) => {
                                    json!({ "uri": uri, "sha256": sha256, "bytes": bytes })
                                }
                                Err(_) => {
                                    json!({ "uri": uri, "sha256": Value::Null, "bytes": bytes, "skipped": "read_error" })
                                }
                            }
                        } else {
                            json!({ "uri": uri, "sha256": Value::Null, "bytes": bytes, "skipped": "too_large" })
                        }
                    }
                    Err(_) => json!({ "uri": uri, "sha256": Value::Null, "skipped": "missing" }),
                };
                files.push(entry);
                continue;
            }
            attachments.push(trimmed.to_string());
        }

        checks = crate::normalize_proof_checks(&checks);
        if checks.is_empty() && attachments.is_empty() {
            let result = json!({
                "workspace": workspace.as_str(),
                "job": job_id,
                "skipped": true,
                "reason": "no proof refs found"
            });
            return ai_ok_with_warnings(
                "tasks_jobs_proof_attach",
                result,
                vec![warning(
                    "PROOF_FROM_JOB_EMPTY",
                    "no proof refs found on job",
                    "Ensure the job summary/refs include CMD:/LINK: receipts or pass artifact_ref.",
                )],
                Vec::new(),
            );
        }

        let mut evidence_args = serde_json::Map::new();
        evidence_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        if let Some(task) = task.clone() {
            evidence_args.insert("task".to_string(), Value::String(task));
        }
        if let Some(step_id) = step_id.clone() {
            evidence_args.insert("step_id".to_string(), Value::String(step_id));
        }
        if let Some(path) = path.clone() {
            evidence_args.insert("path".to_string(), Value::String(path.to_string()));
        }
        if !checks.is_empty() {
            evidence_args.insert(
                "checks".to_string(),
                Value::Array(checks.iter().map(|v| Value::String(v.clone())).collect()),
            );
        }
        if !attachments.is_empty() {
            let mut note = attachments.join("\n");
            if note.len() > 4000 {
                note.truncate(4000);
            }
            evidence_args.insert("note".to_string(), Value::String(note));
        }

        let checkpoint = args_obj.get("checkpoint").cloned().filter(|v| !v.is_null());
        if let Some(checkpoint) = checkpoint {
            evidence_args.insert("checkpoint".to_string(), checkpoint);
        } else {
            evidence_args.insert("checkpoint".to_string(), Value::String("tests".to_string()));
        }

        let evidence_resp = self.tool_tasks_evidence_capture(Value::Object(evidence_args));
        if !evidence_resp
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return evidence_resp;
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "job": job_id,
            "attached": true
        });
        if let Some(obj) = result.as_object_mut() {
            if !files.is_empty() {
                obj.insert("files".to_string(), Value::Array(files));
            }
            if let Some(event) = evidence_resp.get("result").and_then(|v| v.get("event")) {
                obj.insert("event".to_string(), event.clone());
            }
            if let Some(revision) = evidence_resp.get("result").and_then(|v| v.get("revision")) {
                obj.insert("revision".to_string(), revision.clone());
            }
        }

        ai_ok("tasks_jobs_proof_attach", result)
    }
}
