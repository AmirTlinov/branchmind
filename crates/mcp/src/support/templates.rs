#![forbid(unsafe_code)]

use bm_core::model::TaskKind;
use bm_storage::ProofMode;

#[derive(Clone)]
pub(crate) struct TaskTemplateStep {
    pub(crate) title: String,
    pub(crate) success_criteria: Vec<String>,
    pub(crate) tests: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) proof_tests_mode: ProofMode,
    pub(crate) proof_security_mode: ProofMode,
    pub(crate) proof_perf_mode: ProofMode,
    pub(crate) proof_docs_mode: ProofMode,
}

#[derive(Clone)]
pub(crate) struct TaskTemplate {
    pub(crate) id: &'static str,
    pub(crate) kind: TaskKind,
    pub(crate) title: &'static str,
    pub(crate) description: &'static str,
    pub(crate) plan_steps: Vec<String>,
    pub(crate) task_steps: Vec<TaskTemplateStep>,
}

fn template_step(
    title: &str,
    success_criteria: &[&str],
    tests: &[&str],
    blockers: &[&str],
) -> TaskTemplateStep {
    TaskTemplateStep {
        title: title.to_string(),
        success_criteria: success_criteria.iter().map(|s| s.to_string()).collect(),
        tests: tests.iter().map(|s| s.to_string()).collect(),
        blockers: blockers.iter().map(|s| s.to_string()).collect(),
        proof_tests_mode: ProofMode::Off,
        proof_security_mode: ProofMode::Off,
        proof_perf_mode: ProofMode::Off,
        proof_docs_mode: ProofMode::Off,
    }
}

pub(crate) fn built_in_task_templates() -> Vec<TaskTemplate> {
    vec![
        TaskTemplate {
            id: "basic-plan",
            kind: TaskKind::Plan,
            title: "Basic plan checklist",
            description: "Minimal plan checklist for execution.",
            plan_steps: vec![
                "Clarify goal and constraints".to_string(),
                "Implement the change".to_string(),
                "Verify and document".to_string(),
            ],
            task_steps: Vec::new(),
        },
        TaskTemplate {
            id: "principal-plan",
            kind: TaskKind::Plan,
            title: "Principal initiative plan",
            description: "Long-horizon initiative checklist for principal-level delivery.",
            plan_steps: vec![
                "Define goal, scope, and non-goals".to_string(),
                "Capture constraints, stakeholders, and risks".to_string(),
                "Define success criteria and measurable outcomes".to_string(),
                "Explore alternatives and choose an architecture".to_string(),
                "Plan milestones and sequencing (MVP → iterations)".to_string(),
                "Execute implementation in small verified slices".to_string(),
                "Verification: tests, safety, docs, performance".to_string(),
                "Rollout plan: deployment, monitoring, rollback".to_string(),
                "Post-ship review: follow-ups and tech debt".to_string(),
            ],
            task_steps: Vec::new(),
        },
        TaskTemplate {
            id: "basic-task",
            kind: TaskKind::Task,
            title: "Basic delivery task",
            description: "Clarify → implement → verify with explicit checkpoints.",
            plan_steps: Vec::new(),
            task_steps: vec![
                template_step(
                    "Clarify goal and constraints",
                    &["Goal and constraints captured", "Success criteria defined"],
                    &["Planning notes updated"],
                    &[],
                ),
                template_step(
                    "Implement change",
                    &["Implementation complete", "Review for correctness"],
                    &["Relevant tests updated or added"],
                    &[],
                ),
                template_step(
                    "Verify and document",
                    &["Checks executed and passing", "Docs updated if needed"],
                    &["Run relevant test suite"],
                    &[],
                ),
            ],
        },
        TaskTemplate {
            id: "principal-task",
            kind: TaskKind::Task,
            title: "Principal delivery task",
            description: "Principal-grade loop: frame → design → implement → verify → ship, with explicit proofs (defaults to strict reasoning discipline).",
            plan_steps: Vec::new(),
            task_steps: vec![
                template_step(
                    "Frame the problem (goal, constraints, success criteria)",
                    &[
                        "Goal and constraints captured",
                        "Success criteria and risks are explicit",
                    ],
                    &["Seed a brief reasoning capsule (frame/hypothesis)"],
                    &[],
                ),
                template_step(
                    "Design the approach (alternatives, interfaces, invariants)",
                    &[
                        "Alternatives considered and trade-offs recorded",
                        "Interfaces and invariants documented",
                    ],
                    &["Identify the smallest safe probe / prototype"],
                    &[],
                ),
                template_step(
                    "Implement incrementally (small slices)",
                    &["Core change implemented", "No silent behavior changes"],
                    &["Add or update targeted tests"],
                    &[],
                ),
                {
                    let mut step = template_step(
                        "Verify with proofs (tests, docs, safety, perf)",
                        &[
                            "Relevant checks executed and passing",
                            "Docs updated where needed",
                        ],
                        &["Run relevant test suite / checks"],
                        &[],
                    );
                    step.proof_tests_mode = ProofMode::Require;
                    step
                },
                template_step(
                    "Rollout and handoff",
                    &[
                        "Rollback plan defined (if applicable)",
                        "Handoff capsule is complete",
                    ],
                    &["Produce a bounded snapshot for the next agent"],
                    &[],
                ),
            ],
        },
        TaskTemplate {
            id: "flagship-task",
            kind: TaskKind::Task,
            title: "Flagship delivery task",
            description: "Flagship-grade loop: deep planning + branching reasoning → resolved synthesis → incremental delivery → proofs → knowledge handoff. (Defaults to deep reasoning discipline.)",
            plan_steps: Vec::new(),
            task_steps: vec![
                template_step(
                    "Frame the problem (goal, constraints, success criteria, stop criteria)",
                    &[
                        "Goal and constraints captured",
                        "Success + stop criteria are explicit",
                    ],
                    &["Seed a brief reasoning capsule (frame + skeptic preflight)"],
                    &[],
                ),
                template_step(
                    "Branch the reasoning (2+ hypotheses, counter-case, falsifiers)",
                    &[
                        "At least 2 hypotheses captured (best + alternative)",
                        "Counter-position is steelmanned",
                    ],
                    &["Add the smallest disconfirming test idea per hypothesis"],
                    &[],
                ),
                template_step(
                    "Resolve a synthesis decision (tradeoffs, rollback, invariants)",
                    &[
                        "A resolved decision exists (winner + tradeoffs)",
                        "Interfaces and invariants documented",
                    ],
                    &["Record rollback/stop rule and what would change your mind"],
                    &[],
                ),
                template_step(
                    "Implement incrementally (small verified slices)",
                    &["Core change implemented", "No silent behavior changes"],
                    &["Add or update targeted tests"],
                    &[],
                ),
                {
                    let mut step = template_step(
                        "Verify with proofs (tests, docs, safety, perf)",
                        &[
                            "Relevant checks executed and passing",
                            "Docs updated where needed",
                        ],
                        &["Run relevant test suite / checks"],
                        &[],
                    );
                    step.proof_tests_mode = ProofMode::Require;
                    step
                },
                template_step(
                    "Knowledge + handoff",
                    &[
                        "Knowledge cards updated (invariants + pitfalls)",
                        "Next actions and follow-ups captured",
                    ],
                    &["Produce a bounded resume capsule for the next agent"],
                    &[],
                ),
            ],
        },
    ]
}

pub(crate) fn find_task_template(id: &str, kind: TaskKind) -> Option<TaskTemplate> {
    built_in_task_templates()
        .into_iter()
        .find(|template| template.id == id && template.kind == kind)
}

pub(crate) fn find_task_template_any(id: &str) -> Option<TaskTemplate> {
    built_in_task_templates()
        .into_iter()
        .find(|template| template.id == id)
}
