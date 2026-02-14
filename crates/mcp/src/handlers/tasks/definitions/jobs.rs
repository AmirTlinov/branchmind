#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn jobs_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_jobs_create",
            "description": "Create a delegation job (does not execute anything).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "title": { "type": "string" },
                    "prompt": { "type": "string" },
                    "kind": { "type": "string" },
                    "priority": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "executor": { "type": "string", "enum": ["codex", "claude_code", "auto"] },
                    "executor_profile": { "type": "string", "enum": ["fast", "deep", "audit", "xhigh"] },
                    "executor_model": { "type": "string", "description": "default: gpt-5.3-codex" },
                    "policy": {
                        "type": "object",
                        "properties": {
                            "prefer": { "type": "array", "items": { "type": "string" } },
                            "forbid": { "type": "array", "items": { "type": "string" } },
                            "min_profile": { "type": "string" }
                        }
                    },
                    "expected_artifacts": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "title", "prompt"]
            }
        }),
        json!({
            "name": "tasks_jobs_list",
            "description": "List delegation jobs (bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "status": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_artifact_put",
            "description": "Store/update a small job-local artifact (bounded, upsert).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "artifact_key": { "type": "string" },
                    "content_text": { "type": "string" }
                },
                "required": ["workspace", "job", "artifact_key", "content_text"]
            }
        }),
        json!({
            "name": "tasks_jobs_artifact_get",
            "description": "Read a job artifact (bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "artifact_key": { "type": "string" },
                    "offset": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "job", "artifact_key"]
            }
        }),
        json!({
            "name": "tasks_jobs_radar",
            "description": "Radar: list active jobs with a low-noise attention hint (optional reply shortcut).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "status": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "limit": { "type": "integer" },
                    "runners_limit": { "type": "integer" },
                    "runners_status": { "type": "string", "description": "optional filter: idle|live" },
                    "offline_limit": { "type": "integer", "description": "max recent offline runners to include (0 disables section)" },
                    "include_offline": { "type": "boolean", "description": "include recent offline runners section (default=true)" },
                    "stall_after_s": { "type": "integer", "description": "seconds without meaningful progress/checkpoint to mark RUNNING job as stalled (default=600)" },
                    "stale_after_s": { "type": "integer", "description": "deprecated; use stall_after_s" },
                    "reply_job": { "type": "string" },
                    "reply_message": { "type": "string" },
                    "reply_refs": { "type": "array", "items": { "type": "string" } },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_open",
            "description": "Open a job (status + spec + recent events).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "include_artifacts": { "type": "boolean", "description": "Include job_artifacts list (keys + refs) when available (default=false)." },
                    "include_prompt": { "type": "boolean" },
                    "include_events": { "type": "boolean" },
                    "include_meta": { "type": "boolean" },
                    "max_events": { "type": "integer" },
                    "before_seq": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_control_center",
            "description": "Manager control center: one-call ops snapshot + action packages (jobs + inbox + mesh).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "scope": {
                        "type": "object",
                        "properties": {
                            "workspace": { "type": "string" },
                            "task": { "type": "string" },
                            "anchor": { "type": "string" }
                        }
                    },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "view": { "type": "string", "enum": ["smart", "audit"] },
                    "limit": { "type": "integer" },
                    "stall_after_s": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_proof_attach",
            "description": "Attach proof receipts from a job to a task/step (evidence capture).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "task": { "type": "string" },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "checkpoint": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "artifact_ref": { "type": "string" },
                    "max_refs": { "type": "integer" },
                    "max_file_bytes": { "type": "integer", "description": "Max bytes to hash per local file when emitting sha256 (bounded, best-effort)." }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_tail",
            "description": "Tail job events incrementally (seq > after_seq).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "after_seq": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_claim",
            "description": "Claim a job (QUEUED -> RUNNING). Can optionally reclaim stale RUNNING jobs.",
            "inputSchema": {
                "type": "object",
                "oneOf": [
                    { "required": ["workspace", "job", "runner_id"] },
                    { "required": ["workspace", "job", "runner"] }
                ],
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string", "description": "stable runner identity (recommended)" },
                    "runner": { "type": "string", "description": "deprecated alias for runner_id" },
                    "allow_stale": { "type": "boolean" },
                    "lease_ttl_ms": { "type": "integer", "description": "claim lease TTL; renewed by tasks_jobs_report heartbeats" }
                }
            }
        }),
        json!({
            "name": "tasks_jobs_message",
            "description": "Send a manager message to a job (QUEUED/RUNNING).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "message": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job", "message"]
            }
        }),
        json!({
            "name": "tasks_jobs_report",
            "description": "Append a progress event to a running job (bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "claim_revision": { "type": "integer" },
                    "lease_ttl_ms": { "type": "integer" },
                    "kind": { "type": "string" },
                    "message": { "type": "string" },
                    "percent": { "type": "integer" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job", "runner_id", "claim_revision", "message"],
                "allOf": [
                    {
                        "if": {
                            "properties": {
                                "kind": { "enum": ["progress", "checkpoint"] }
                            },
                            "required": ["kind"]
                        },
                        "then": {
                            "required": ["meta"],
                            "properties": {
                                "meta": {
                                    "type": "object",
                                    "required": ["step"],
                                    "properties": {
                                        "step": {
                                            "type": "object",
                                            "required": ["command"],
                                            "properties": {
                                                "command": { "type": "string", "minLength": 1 },
                                                "result": { "not": { "type": "null" } },
                                                "error": { "not": { "type": "null" } }
                                            },
                                            "anyOf": [
                                                { "required": ["result"] },
                                                { "required": ["error"] }
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                ]
            }
        }),
        json!({
            "name": "tasks_jobs_complete",
            "description": "Complete a job (DONE/FAILED/CANCELED) and attach stable refs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "claim_revision": { "type": "integer" },
                    "status": { "type": "string" },
                    "summary": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job", "runner_id", "claim_revision", "status"]
            }
        }),
        json!({
            "name": "tasks_jobs_requeue",
            "description": "Requeue a terminal job back to QUEUED (bounded, audit event).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "reason": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_rotate_stalled",
            "description": "Manager macro: rotate stalled RUNNING jobs (cancel + recreate).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "stall_after_s": { "type": "integer", "description": "seconds without meaningful checkpoint/progress to consider a RUNNING job stalled (default=600)" },
                    "stale_after_s": { "type": "integer", "description": "deprecated; use stall_after_s" },
                    "limit": { "type": "integer", "description": "max stalled jobs to rotate (default=5)" },
                    "dry_run": { "type": "boolean", "description": "when true, only report what would be rotated" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_respond_inbox",
            "description": "Manager macro: respond to inbox items (questions) with one call (auto-targets needs_manager jobs when job/jobs are omitted).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "jobs": { "type": "array", "items": { "type": "string" } },
                    "message": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "description": "auto-target scan limit when job/jobs are omitted (default=25)" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "message"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_dispatch_slice",
            "description": "Manager macro: dispatch a single slice as a job record (create + routing meta).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "title": { "type": "string" },
                    "prompt": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "priority": { "type": "string" },
                    "executor": { "type": "string", "enum": ["codex", "claude_code", "auto"] },
                    "executor_profile": { "type": "string", "enum": ["fast", "deep", "audit", "xhigh"] },
                    "executor_model": { "type": "string", "description": "default: gpt-5.3-codex" },
                    "policy": { "type": "object" },
                    "expected_artifacts": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "title", "prompt"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_dispatch_scout",
            "description": "Manager macro: dispatch scout stage (claude_code+haiku default, context-only output contract).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "objective": { "type": "string" },
                    "constraints": { "type": "array", "items": { "type": "string" } },
                    "max_context_refs": { "type": "integer" },
                    "executor": { "type": "string", "enum": ["codex", "claude_code"], "description": "default: claude_code" },
                    "executor_profile": { "type": "string", "enum": ["fast", "deep", "audit", "xhigh"], "description": "default: deep for claude_code, xhigh for codex" },
                    "model": { "type": "string", "description": "default for claude_code: haiku (must contain 'haiku'); codex scout must use gpt-5.3-codex" },
                    "quality_profile": { "type": "string", "enum": ["standard", "flagship"] },
                    "novelty_policy": { "type": "string", "enum": ["strict", "warn"] },
                    "critic_pass": { "type": "boolean" },
                    "coverage_targets": { "type": "object" },
                    "max_anchor_overlap": { "type": "number" },
                    "max_ref_redundancy": { "type": "number" },
                    "dry_run": { "type": "boolean" },
                },
                "required": ["workspace", "slice_id"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_dispatch_builder",
            "description": "Manager macro: dispatch builder stage (slice diff batch contract).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "scout_pack_ref": { "type": "string" },
                    "objective": { "type": "string" },
                    "dod": { "type": "object" },
                    "executor": { "type": "string", "enum": ["codex"], "description": "hard pin: codex" },
                    "executor_profile": { "type": "string", "enum": ["xhigh"], "description": "hard pin: xhigh" },
                    "model": { "type": "string", "description": "hard pin: gpt-5.3-codex" },
                    "allow_prevalidate_non_pass": { "type": "boolean", "description": "default false; true allows builder dispatch when scout pre-validate is need_more/reject (not recommended)" },
                    "strict_scout_mode": { "type": "boolean", "description": "default true; enforce stale+quality scout gates before builder dispatch" },
                    "context_quality_gate": { "type": "boolean", "description": "default true; fail-closed on scout contract warnings (including stale CODE_REF)" },
                    "input_mode": { "type": "string", "enum": ["strict", "flex"], "description": "default strict; strict mode forbids builder-side context/tool discovery loops" },
                    "max_context_requests": { "type": "integer", "description": "bounded <=2; alias for context_retry_limit (default 2)" },
                    "scout_stale_after_s": { "type": "integer", "description": "strict mode freshness budget in seconds (default: 900, max: 86400)" },
                    "context_retry_count": { "type": "integer", "description": "current builder context-request retry counter (bounded <=2)" },
                    "context_retry_limit": { "type": "integer", "description": "max builder context-request retries (bounded <=2, default 2)" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "slice_id", "scout_pack_ref"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_dispatch_validator",
            "description": "Manager macro: dispatch validator stage (independent verification contract).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "scout_pack_ref": { "type": "string" },
                    "builder_batch_ref": { "type": "string" },
                    "plan_ref": { "type": "string" },
                    "executor": { "type": "string", "enum": ["claude_code"], "description": "hard pin: claude_code" },
                    "executor_profile": { "type": "string", "enum": ["audit"], "description": "hard pin: audit" },
                    "model": { "type": "string", "description": "hard pin: opus-4.6 family" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "slice_id", "scout_pack_ref", "builder_batch_ref"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_enforce_proof",
            "description": "Manager macro: acknowledge proof gate by posting a manager message with proof refs (auto-targets needs_proof jobs when job/jobs are omitted).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "jobs": { "type": "array", "items": { "type": "string" } },
                    "message": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "description": "auto-target scan limit when job/jobs are omitted (default=25)" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "refs"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_sync_team",
            "description": "Manager macro: publish a shared task plan delta to the team mesh thread.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan_delta": { "type": "object" },
                    "idempotency_key": { "type": "string" },
                    "from_agent_id": { "type": "string" }
                },
                "required": ["workspace", "task", "plan_delta", "idempotency_key"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_ab_slice",
            "description": "Pipeline macro: run scout A/B setup (weak vs strong) and optionally compare validator outcomes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "objective": { "type": "string" },
                    "constraints": { "type": "array", "items": { "type": "string" } },
                    "variant_a": { "type": "object", "properties": { "scout_mode": { "type": "string", "enum": ["weak", "strong"] } } },
                    "variant_b": { "type": "object", "properties": { "scout_mode": { "type": "string", "enum": ["weak", "strong"] } } },
                    "policy": { "type": "string", "enum": ["fail_closed"] },
                    "dry_run": { "type": "boolean" },
                    "validator_report_ref_a": { "type": "string" },
                    "validator_report_ref_b": { "type": "string" }
                },
                "required": ["workspace", "task", "anchor", "slice_id", "objective"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_gate",
            "description": "Pipeline gate: lead decision over scout/builder/validator artifacts (approve|rework|reject).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "scout_pack_ref": { "type": "string" },
                    "builder_batch_ref": { "type": "string" },
                    "validator_report_ref": { "type": "string" },
                    "policy": { "type": "string", "enum": ["fail_closed"] }
                },
                "required": ["workspace", "task", "slice_id", "scout_pack_ref", "builder_batch_ref", "validator_report_ref"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_apply",
            "description": "Pipeline apply: fail-closed execution after lead approve decision.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "decision_ref": { "type": "string" },
                    "builder_batch_ref": { "type": "string" },
                    "expected_revision": { "type": "integer" }
                },
                "required": ["workspace", "task", "slice_id", "decision_ref", "builder_batch_ref", "expected_revision"]
            }
        }),
        json!({
            "name": "tasks_jobs_macro_dispatch_writer",
            "description": "Manager macro: dispatch writer stage (PatchOp output contract, no filesystem writes).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "scout_pack_ref": { "type": "string" },
                    "objective": { "type": "string" },
                    "dod": { "type": "object" },
                    "executor": { "type": "string", "enum": ["codex", "claude_code", "auto"] },
                    "executor_profile": { "type": "string", "enum": ["fast", "deep", "audit", "xhigh"] },
                    "model": { "type": "string", "description": "must be gpt-5.3-codex" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["workspace", "task", "slice_id", "scout_pack_ref", "objective", "dod"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_pre_validate",
            "description": "Pipeline pre-validate: deterministic check of scout pack completeness before writer dispatch (no LLM).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string", "description": "optional context hint for UX/reporting" },
                    "slice_id": { "type": "string", "description": "optional context hint for UX/reporting" },
                    "scout_pack_ref": { "type": "string" }
                },
                "required": ["workspace", "scout_pack_ref"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_context_review",
            "description": "Pipeline context review: fail-closed scout context quality verdict (freshness/coverage/dedupe/traceability) before builder dispatch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "scout_pack_ref": { "type": "string" },
                    "mode": { "type": "string", "enum": ["deterministic", "haiku_fast"] },
                    "policy": { "type": "string", "enum": ["fail_closed"] }
                },
                "required": ["workspace", "task", "slice_id", "scout_pack_ref"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_cascade_init",
            "description": "Pipeline cascade: init multi-stage pipeline session (scout -> pre-validate -> writer -> post-validate -> apply) with retry budget.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "objective": { "type": "string" },
                    "constraints": { "type": "array", "items": { "type": "string" } },
                    "max_context_refs": { "type": "integer" },
                    "quality_profile": { "type": "string", "enum": ["standard", "flagship"] },
                    "novelty_policy": { "type": "string", "enum": ["strict", "warn"] },
                    "dry_run": { "type": "boolean" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "task", "slice_id", "objective"]
            }
        }),
        json!({
            "name": "tasks_jobs_pipeline_cascade_advance",
            "description": "Pipeline cascade: advance the state machine after a stage completes (event-driven, retry-aware).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "session_json": { "type": "object", "description": "cascade session state (from cascade.init or previous advance)" },
                    "event": { "type": "string", "description": "stage completion event (scout_done|pre_validate_pass|pre_validate_need_more|pre_validate_reject|writer_done|approve|writer_retry|scout_retry|escalate)" },
                    "hints": { "type": "array", "items": { "type": "string" }, "description": "feedback hints for retry" },
                    "job_id": { "type": "string", "description": "completed job id for lineage tracking" }
                },
                "required": ["workspace", "session_json", "event"]
            }
        }),
        json!({
            "name": "tasks_jobs_mesh_publish",
            "description": "Team mesh: publish a message into a thread (at-least-once + idempotency).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "thread_id": { "type": "string" },
                    "kind": { "type": "string" },
                    "summary": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "plan_delta": { "type": "object" },
                    "handoff": { "type": "object" },
                    "payload": { "type": "object" },
                    "idempotency_key": { "type": "string" },
                    "from_agent_id": { "type": "string" },
                    "from_job_id": { "type": "string" },
                    "to_agent_id": { "type": "string" }
                },
                "required": ["workspace", "thread_id", "kind", "summary", "idempotency_key"]
            }
        }),
        json!({
            "name": "tasks_jobs_mesh_pull",
            "description": "Team mesh: pull messages after a cursor (cursor-based paging).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "thread_id": { "type": "string" },
                    "consumer_id": { "type": "string" },
                    "after_seq": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "thread_id"]
            }
        }),
        json!({
            "name": "tasks_jobs_mesh_ack",
            "description": "Team mesh: ack a cursor for a consumer (idempotent).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "thread_id": { "type": "string" },
                    "consumer_id": { "type": "string" },
                    "after_seq": { "type": "integer" }
                },
                "required": ["workspace", "thread_id", "after_seq"]
            }
        }),
        json!({
            "name": "tasks_jobs_mesh_link",
            "description": "Team mesh: publish a deterministic dependency edge (thread<->thread) as a link message.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from_thread": { "type": "string" },
                    "to_thread": { "type": "string" },
                    "kind": { "type": "string" },
                    "summary": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "idempotency_key": { "type": "string" },
                    "from_agent_id": { "type": "string" }
                },
                "required": ["workspace", "from_thread", "to_thread", "idempotency_key"]
            }
        }),
        json!({
            "name": "tasks_runner_heartbeat",
            "description": "Runner liveness lease update (explicit live/idle/offline status; used by external runners).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "status": { "type": "string", "description": "idle|live" },
                    "active_job_id": { "type": "string" },
                    "lease_ttl_ms": { "type": "integer" },
                    "executors": { "type": "array", "items": { "type": "string" } },
                    "profiles": { "type": "array", "items": { "type": "string" } },
                    "supports_artifacts": { "type": "array", "items": { "type": "string" } },
                    "max_parallel": { "type": "integer" },
                    "sandbox_policy": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "runner_id", "status"]
            }
        }),
    ]
}
