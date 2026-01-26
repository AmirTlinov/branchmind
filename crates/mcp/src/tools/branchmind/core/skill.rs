#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

const SKILL_PACK_VERSION: &str = "0.1.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkillProfile {
    Daily,
    Strict,
    Research,
    Teamlead,
}

impl SkillProfile {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "daily" => Some(Self::Daily),
            "strict" => Some(Self::Strict),
            "research" => Some(Self::Research),
            "teamlead" => Some(Self::Teamlead),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Strict => "strict",
            Self::Research => "research",
            Self::Teamlead => "teamlead",
        }
    }

    fn section_name(&self) -> &'static str {
        match self {
            Self::Daily => "DAILY",
            Self::Strict => "STRICT",
            Self::Research => "RESEARCH",
            Self::Teamlead => "TEAMLEAD",
        }
    }
}

impl McpServer {
    pub(crate) fn tool_branchmind_skill(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let profile = match optional_string(args_obj, "profile") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let profile = profile
            .as_deref()
            .and_then(SkillProfile::parse)
            .unwrap_or(SkillProfile::Daily);

        let text = render_skill_pack(profile, max_chars);

        let mut resp = ai_ok("skill", Value::String(text));
        if let Some(obj) = resp.as_object_mut() {
            // Skills are read-mostly and should not waste context on JSON envelopes.
            obj.insert("line_protocol".to_string(), Value::Bool(true));
        }
        resp
    }
}

fn render_skill_pack(profile: SkillProfile, max_chars: Option<usize>) -> String {
    let mut out = Vec::<String>::new();

    out.push(format!(
        "skill profile={} version={}",
        profile.as_str(),
        SKILL_PACK_VERSION
    ));

    // Truncation invariant: profile-specific guidance must appear early so even tiny budgets remain useful.
    match profile {
        SkillProfile::Daily => {
            push_section(
                &mut out,
                profile.section_name(),
                &[
                    "Golden path: status → tasks_macro_start → tasks_snapshot.",
                    "When lost: tasks_snapshot (refs=true) → open <ref> → follow 1 next action.",
                    "Write to meaning: use anchors (a:<slug>) for decisions/evidence/tests.",
                ],
            );
        }
        SkillProfile::Strict => {
            push_section(
                &mut out,
                profile.section_name(),
                &[
                    "Loop: tasks_snapshot → tasks_lint (patches_limit=2) → apply 1 patch → think_card (hypothesis+test) → evidence → close_step.",
                    "DoD discipline: every active step has success_criteria + tests + proof.",
                    "Override exists but must be explicit: reason+risk (visible debt).",
                ],
            );
        }
        SkillProfile::Research => {
            push_section(
                &mut out,
                profile.section_name(),
                &[
                    "Unit of progress: hypothesis → falsifier test → evidence → decision (canon).",
                    "Add stop criteria (time/budget/signal) to avoid infinite loops.",
                    "Keep research anchor-scoped + step-scoped so you can resume without scanning.",
                ],
            );
        }
        SkillProfile::Teamlead => {
            push_section(
                &mut out,
                profile.section_name(),
                &[
                    "Fan-out: split work into 3–10 jobs by anchors; keep each job bounded.",
                    "Inbox loop: tasks_jobs_radar → open JOB ref → answer once → job continues.",
                    "Accept DONE only with proof refs (or explicit override with reason+risk).",
                    "Fan-in: produce one canonical merge report (what changed + proofs + risks).",
                    "Liveness is explicit: runner state is live/idle/offline; reclaim offline slices when needed.",
                ],
            );
        }
    }

    push_section(
        &mut out,
        "CORE LOOP",
        &[
            "Read tasks_snapshot (BM‑L1 compass) → do 1 next action → leave 1 receipt → repeat.",
            "Keep 1 primary next action + 1 backup. Everything else is hidden by default.",
            "Never “hunt chat”: every important thing must have a ref you can open.",
        ],
    );

    push_section(
        &mut out,
        "PROOF",
        &[
            "Prefer receipts: CMD: ... (what you ran) + LINK: ... (CI/artifact/log).",
            "If a step requires proof: DONE without proof is rejected unless you override (reason+risk).",
            "If you only have narrative text: convert it into explicit receipts or a single merge report with refs.",
        ],
    );

    push_section(
        &mut out,
        "ANCHORS",
        &[
            "Use anchors as meaning coordinates (a:<slug>), not file paths.",
            "Bind decisions/evidence/tests to anchors so new sessions resume by meaning in seconds.",
            "Keep the map clean: merge/rename anchors instead of proliferating near-duplicates.",
        ],
    );

    let mut text = out.join("\n");
    if let Some(limit) = max_chars {
        let (limit, _clamped) = clamp_budget_max(limit);
        if text.len() > limit {
            let suffix = "...";
            let budget = limit.saturating_sub(suffix.len());
            text = truncate_string_bytes(&text, budget) + suffix;
        }
    }
    text
}

fn push_section(out: &mut Vec<String>, name: &str, lines: &[&str]) {
    let mut body = Vec::<String>::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        body.push(trimmed.to_string());
    }
    if body.is_empty() {
        return;
    }

    out.push(format!("[{name}]"));
    out.extend(body);
}
