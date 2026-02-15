#![forbid(unsafe_code)]

use crate::support::ai_error_with;
use serde::Serialize;
use serde_json::Value;

use super::{
    PlanFsPlan, PlanFsPlanYaml, PlanFsPlanYamlHeader, PlanFsSectionBundle, PlanFsSlice,
    PlanFsSliceRef, PlanFsSliceYaml, PlanFsSliceYamlHeader,
};

type SectionAccessor = fn(&PlanFsSectionBundle) -> &[String];

const SECTION_ORDER: &[(&str, SectionAccessor)] = &[
    ("Goal", |bundle| std::slice::from_ref(&bundle.goal)),
    ("Scope", |bundle| &bundle.scope),
    ("Non-goals", |bundle| &bundle.non_goals),
    ("Interfaces", |bundle| &bundle.interfaces),
    ("Contracts", |bundle| &bundle.contracts),
    ("Tests", |bundle| &bundle.tests),
    ("Proof", |bundle| &bundle.proof),
    ("Rollback", |bundle| &bundle.rollback),
    ("Risks", |bundle| &bundle.risks),
];

pub(crate) fn render_plan_markdown(plan: &PlanFsPlan) -> Result<String, Value> {
    let legend = render_planfs_yaml(&build_plan_yaml(plan))?;
    let mut out = String::new();
    out.push_str("[LEGEND]\n");
    out.push_str(&legend);
    out.push_str("[CONTENT]\n");
    out.push_str(&render_sections(&plan.sections));
    Ok(out)
}

pub(crate) fn render_slice_markdown(slice: &PlanFsSlice) -> Result<String, Value> {
    let legend = render_planfs_yaml(&build_slice_yaml(slice))?;
    let mut out = String::new();
    out.push_str("[LEGEND]\n");
    out.push_str(&legend);
    out.push_str("[CONTENT]\n");
    out.push_str(&render_sections(&slice.sections));
    Ok(out)
}

#[allow(dead_code)]
pub(crate) fn render_plan_files_manifest(plan: &PlanFsPlan) -> Vec<&PlanFsSliceRef> {
    let mut out = Vec::with_capacity(plan.slices.len());
    for item in &plan.slices {
        out.push(item);
    }
    out
}

fn build_plan_yaml(plan: &PlanFsPlan) -> PlanFsPlanYaml {
    PlanFsPlanYaml {
        planfs_v1: PlanFsPlanYamlHeader {
            plan_slug: plan.plan_slug.clone(),
            title: plan.title.clone(),
            objective: plan.objective.clone(),
            constraints: plan.constraints.clone(),
            policy: plan.policy.clone(),
            slices: plan
                .slices
                .iter()
                .map(|slice_ref| super::PlanFsSliceYamlRef {
                    id: slice_ref.id.clone(),
                    title: slice_ref.title.clone(),
                    file: slice_ref.file.clone(),
                    status: slice_ref.status.clone(),
                })
                .collect(),
        },
    }
}

fn build_slice_yaml(slice: &PlanFsSlice) -> PlanFsSliceYaml {
    PlanFsSliceYaml {
        planfs_v1: PlanFsSliceYamlHeader {
            id: slice.id.clone(),
            title: slice.title.clone(),
            objective: slice.objective.clone(),
            status: slice.status.clone(),
            budgets: slice.budgets.clone(),
            dod: slice.dod.clone(),
            tasks: slice.tasks.clone(),
        },
    }
}

fn render_planfs_yaml<T>(value: &T) -> Result<String, Value>
where
    T: Serialize,
{
    let yaml = serde_yaml::to_string(value).map_err(|err| {
        ai_error_with(
            "STORE_ERROR",
            &format!("failed to serialize planfs front matter: {err}"),
            Some("Use plain UTF-8 strings without unsupported YAML values"),
            vec![],
        )
    })?;
    Ok(format!("{yaml}\n"))
}

fn render_sections(bundle: &PlanFsSectionBundle) -> String {
    let mut out = String::new();
    for (title, accessor) in SECTION_ORDER {
        out.push_str(&format!("## {title}\n"));
        let value = accessor(bundle);
        if value.len() == 1 {
            let single = value[0].trim();
            if !single.is_empty() && title.eq_ignore_ascii_case("Goal") {
                out.push_str(single);
                out.push('\n');
                continue;
            }
        }
        if value.is_empty() {
            continue;
        }
        for item in value {
            out.push_str(&format!("- {}\n", item.trim()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::planfs::{
        PlanFsReadLimits, parse_plan_with_front_matter, parse_slice_with_front_matter,
    };

    fn sample_plan() -> PlanFsPlan {
        PlanFsPlan {
            plan_slug: "sample-plan".to_string(),
            title: "sample plan".to_string(),
            objective: "sample objective".to_string(),
            constraints: vec!["No placeholders".to_string()],
            policy: "strict".to_string(),
            slices: vec![PlanFsSliceRef {
                id: "SLICE-1".to_string(),
                title: "Slice 1".to_string(),
                file: "Slice-1.md".to_string(),
                status: Some("todo".to_string()),
            }],
            sections: PlanFsSectionBundle {
                goal: "Deliver one change".to_string(),
                scope: vec!["scope-1".to_string()],
                non_goals: vec!["non-goal-1".to_string()],
                interfaces: vec!["interface-1".to_string()],
                contracts: vec!["contract-1".to_string()],
                tests: vec!["test-1".to_string()],
                proof: vec!["proof-1".to_string()],
                rollback: vec!["rollback-1".to_string()],
                risks: vec!["risk-1".to_string()],
            },
        }
    }

    fn sample_slice() -> PlanFsSlice {
        let steps = vec![
            crate::support::planfs::PlanFsStep {
                title: "Step 1".to_string(),
                success_criteria: vec!["SC1".to_string()],
                tests: vec!["T1".to_string()],
                blockers: vec!["B1".to_string()],
                rollback: vec!["R1".to_string()],
            },
            crate::support::planfs::PlanFsStep {
                title: "Step 2".to_string(),
                success_criteria: vec!["SC2".to_string()],
                tests: vec!["T2".to_string()],
                blockers: vec!["B2".to_string()],
                rollback: vec!["R2".to_string()],
            },
            crate::support::planfs::PlanFsStep {
                title: "Step 3".to_string(),
                success_criteria: vec!["SC3".to_string()],
                tests: vec!["T3".to_string()],
                blockers: vec!["B3".to_string()],
                rollback: vec!["R3".to_string()],
            },
        ];

        PlanFsSlice {
            id: "SLICE-1".to_string(),
            title: "Slice 1".to_string(),
            objective: "sample objective".to_string(),
            status: Some("todo".to_string()),
            budgets: crate::support::planfs::PlanFsBudgets {
                max_files: 10,
                max_diff_lines: 200,
                max_context_refs: 20,
            },
            dod: crate::support::planfs::PlanFsDod {
                success_criteria: vec!["DoD".to_string()],
                tests: vec!["t".to_string()],
                blockers: vec!["b".to_string()],
                rollback: vec!["r".to_string()],
            },
            tasks: vec![
                crate::support::planfs::PlanFsTask {
                    title: "Task 1".to_string(),
                    success_criteria: vec!["SC1".to_string()],
                    tests: vec!["T1".to_string()],
                    blockers: vec!["B1".to_string()],
                    rollback: vec!["R1".to_string()],
                    steps: steps.clone(),
                },
                crate::support::planfs::PlanFsTask {
                    title: "Task 2".to_string(),
                    success_criteria: vec!["SC2".to_string()],
                    tests: vec!["T2".to_string()],
                    blockers: vec!["B2".to_string()],
                    rollback: vec!["R2".to_string()],
                    steps: steps.clone(),
                },
                crate::support::planfs::PlanFsTask {
                    title: "Task 3".to_string(),
                    success_criteria: vec!["SC3".to_string()],
                    tests: vec!["T3".to_string()],
                    blockers: vec!["B3".to_string()],
                    rollback: vec!["R3".to_string()],
                    steps,
                },
            ],
            sections: PlanFsSectionBundle {
                goal: "Slice goal".to_string(),
                scope: vec!["scope-1".to_string()],
                non_goals: vec!["non-goal-1".to_string()],
                interfaces: vec!["interface-1".to_string()],
                contracts: vec!["contract-1".to_string()],
                tests: vec!["test-1".to_string()],
                proof: vec!["proof-1".to_string()],
                rollback: vec!["rollback-1".to_string()],
                risks: vec!["risk-1".to_string()],
            },
        }
    }

    #[test]
    fn render_plan_roundtrip_is_stable() {
        let original = sample_plan();
        let rendered = render_plan_markdown(&original).expect("render plan");
        let (parsed, _refs) =
            parse_plan_with_front_matter(&rendered, true, &PlanFsReadLimits::default())
                .expect("parse rendered plan");
        let rendered_two = render_plan_markdown(&parsed).expect("render plan second");
        assert_eq!(rendered, rendered_two);
    }

    #[test]
    fn render_slice_roundtrip_is_stable() {
        let original = sample_slice();
        let rendered = render_slice_markdown(&original).expect("render slice");
        let parsed = parse_slice_with_front_matter(&rendered, true, &PlanFsReadLimits::default())
            .expect("parse rendered slice");
        let rendered_two = render_slice_markdown(&parsed).expect("render slice second");
        assert_eq!(rendered, rendered_two);
    }
}
