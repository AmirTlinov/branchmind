#![forbid(unsafe_code)]

use super::{SliceBudgets, SliceDod, SlicePlanSpec, SliceStepSpec, SliceTaskSpec};

pub(crate) fn propose_next_slice_spec(
    plan_id: &str,
    plan_title: &str,
    objective: &str,
    constraints: &[String],
) -> SlicePlanSpec {
    let constraint_text = if constraints.is_empty() {
        vec![
            "Keep scope strictly within one reviewable slice.".to_string(),
            "No utility-junk or speculative abstractions.".to_string(),
            "All tests and rollback path must be explicit.".to_string(),
        ]
    } else {
        constraints.to_vec()
    };
    let objective_trimmed = objective.trim();
    let objective_final = if objective_trimmed.is_empty() {
        format!("Advance plan {plan_id}: {plan_title}")
    } else {
        objective_trimmed.to_string()
    };
    let objective_for_tasks = objective_final.clone();
    let base_title = if plan_title.trim().is_empty() {
        format!("Slice for {plan_id}")
    } else {
        format!("Slice for {plan_title}")
    };

    let make_task = |idx: usize, label: &str| -> SliceTaskSpec {
        let prefix = format!("{}.{idx}", plan_id.trim());
        SliceTaskSpec {
            title: format!("{label}: {} ({prefix})", objective_for_tasks),
            success_criteria: vec![
                format!("{label} scope is delivered with explicit boundaries."),
                format!("{label} has deterministic pass/fail evidence."),
            ],
            tests: vec![
                format!("{label}: unit/contract checks pass"),
                format!("{label}: regression checks pass"),
            ],
            blockers: vec![
                "No quick-fixes or hidden side effects.".to_string(),
                "No scope expansion outside this slice.".to_string(),
            ],
            steps: vec![
                SliceStepSpec {
                    title: format!("{label} — define exact contracts and invariants"),
                    success_criteria: vec![
                        "Contracts are explicit and bounded.".to_string(),
                        "Unknowns are listed with falsifier.".to_string(),
                    ],
                    tests: vec!["Contract fixtures updated".to_string()],
                    blockers: vec!["No ambiguous acceptance criteria.".to_string()],
                },
                SliceStepSpec {
                    title: format!("{label} — implement minimal cohesive change"),
                    success_criteria: vec![
                        "Change is minimal and reviewable.".to_string(),
                        "Architecture boundaries remain intact.".to_string(),
                    ],
                    tests: vec!["Implementation compiles and targeted tests pass".to_string()],
                    blockers: vec!["No duplicated logic.".to_string()],
                },
                SliceStepSpec {
                    title: format!("{label} — validate, prove, and prepare rollback"),
                    success_criteria: vec![
                        "Evidence collected for DoD and policy.".to_string(),
                        "Rollback command/path verified.".to_string(),
                    ],
                    tests: vec!["Smoke/regression checks recorded".to_string()],
                    blockers: vec!["No close without proof refs.".to_string()],
                },
            ],
        }
    };

    SlicePlanSpec {
        title: base_title,
        objective: objective_final,
        non_goals: constraint_text,
        shared_context_refs: vec![format!("PLAN:{plan_id}")],
        dod: SliceDod {
            criteria: vec![
                "Slice outcome is directly tied to plan objective.".to_string(),
                "Implementation stays within declared budgets.".to_string(),
            ],
            tests: vec![
                "All slice-level checks listed in tasks are executed.".to_string(),
                "No regression in touched area.".to_string(),
            ],
            blockers: vec![
                "No overengineering and no scaffold-only code.".to_string(),
                "No hidden behavior changes outside slice scope.".to_string(),
            ],
        },
        tasks: vec![
            make_task(1, "Context and design"),
            make_task(2, "Implementation"),
            make_task(3, "Validation and readiness"),
        ],
        budgets: SliceBudgets::default(),
    }
}
