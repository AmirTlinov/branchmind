#![forbid(unsafe_code)]

use super::super::ai_error;
use super::SlicePlanSpec;
use bm_storage::StepListRow;
use serde_json::Value;

fn step_path_depth(path: &str) -> Result<(usize, Vec<usize>), Value> {
    let parsed = crate::StepPath::parse(path)
        .map_err(|_| ai_error("PRECONDITION_FAILED", "invalid step path inside slice task"))?;
    let indices = parsed.indices().to_vec();
    Ok((indices.len(), indices))
}

pub(crate) fn validate_slice_step_tree(
    steps: &[StepListRow],
    spec: &SlicePlanSpec,
) -> Result<(), Value> {
    if steps.is_empty() {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "slice has no steps; expected SliceTasks(root) + Steps(children)",
        ));
    }

    // Fail-closed: slices are exactly 2-level trees (root SliceTasks, child Steps).
    let mut roots = std::collections::BTreeMap::<usize, &StepListRow>::new();
    let mut children = std::collections::BTreeMap::<usize, Vec<&StepListRow>>::new();
    for row in steps {
        let (depth, indices) = step_path_depth(&row.path)?;
        if depth == 1 {
            let root_idx = indices[0];
            roots.insert(root_idx, row);
        } else if depth == 2 {
            let root_idx = indices[0];
            children.entry(root_idx).or_default().push(row);
        } else {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice step tree depth must be <=2 (SliceTasks -> Steps)",
            ));
        }
    }

    let expected_roots = spec.tasks.len();
    if roots.len() != expected_roots {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "slice root steps count does not match slice_plan_spec.tasks length",
        ));
    }

    for idx in 0..expected_roots {
        let Some(root_row) = roots.get(&idx) else {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice root step ordinals must be contiguous starting at s:0",
            ));
        };
        let expected_title = spec.tasks[idx].title.trim();
        if !expected_title.is_empty() && root_row.title.trim() != expected_title {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice root step titles drifted from slice_plan_spec (determinism violated)",
            ));
        }
        let expected_children = spec.tasks[idx].steps.len();
        let actual_children = children.get(&idx).map(|v| v.len()).unwrap_or(0);
        if actual_children != expected_children {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice child step count does not match slice_plan_spec task.steps length",
            ));
        }

        // Validate child ordinals contiguous.
        let mut ordinals = match children.get(&idx) {
            Some(rows) => rows
                .iter()
                .filter_map(|row| {
                    step_path_depth(&row.path)
                        .ok()
                        .and_then(|(_, indices)| indices.get(1).copied())
                })
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        ordinals.sort();
        ordinals.dedup();
        if ordinals.len() != expected_children {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice child step ordinals must be unique and contiguous",
            ));
        }

        for (expected, got) in (0..expected_children).zip(ordinals.iter().copied()) {
            if expected != got {
                return Err(ai_error(
                    "PRECONDITION_FAILED",
                    "slice child step ordinals must start at s:0 and be contiguous",
                ));
            }
        }

        // Validate child titles match spec.
        if let Some(rows) = children.get(&idx) {
            for row in rows {
                let (_, indices) = step_path_depth(&row.path)?;
                let child_idx = indices[1];
                let expected_title = spec.tasks[idx].steps[child_idx].title.trim();
                if !expected_title.is_empty() && row.title.trim() != expected_title {
                    return Err(ai_error(
                        "PRECONDITION_FAILED",
                        "slice child step titles drifted from slice_plan_spec (determinism violated)",
                    ));
                }
            }
        }
    }

    // Guard against hidden extra steps: total steps must match spec exactly.
    let expected_total = expected_roots + spec.tasks.iter().map(|t| t.steps.len()).sum::<usize>();
    if steps.len() != expected_total {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "slice step tree contains extra steps beyond slice_plan_spec (scope blowup)",
        ));
    }
    Ok(())
}
