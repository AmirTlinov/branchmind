#![forbid(unsafe_code)]

use crate::support::{ai_error, ai_error_with};
use serde_json::Value;
use serde_yaml::Value as YamlValue;
use std::collections::BTreeMap;

use super::{
    MAX_SLICE_TASKS, MAX_STEP_LIST_LEN, MIN_SLICE_TASKS, MIN_STEP_LIST_LEN, PlanFsBudgets,
    PlanFsDod, PlanFsPlan, PlanFsPlanYaml, PlanFsPlanYamlHeader, PlanFsReadLimits,
    PlanFsSectionBundle, PlanFsSlice, PlanFsSliceRef, PlanFsSliceYaml, PlanFsStep, PlanFsTask,
};

const LEGEND_MARKER: &str = "[LEGEND]";
const CONTENT_MARKER: &str = "[CONTENT]";

const PLAN_REQUIRED_SECTIONS: &[&str] = &[
    "goal",
    "scope",
    "non-goals",
    "interfaces",
    "contracts",
    "tests",
    "proof",
    "rollback",
    "risks",
];

const PLACEHOLDER_PATTERNS: [&str; 10] = [
    "todo",
    "tbd",
    "placeholder",
    "<todo>",
    "<tbd>",
    "<fill>",
    "fill me",
    "...",
    "<fill",
    "<todo",
];

pub(crate) fn parse_plan_with_front_matter(
    raw: &str,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<(PlanFsPlan, Vec<PlanFsSliceRef>), Value> {
    let (legend, content) = split_file(raw)?;
    let yaml_value: YamlValue = serde_yaml::from_str(legend).map_err(|err| {
        ai_error_with(
            "INVALID_INPUT",
            &format!("PLAN.md [LEGEND] must be valid YAML: {err}"),
            Some("Fix front-matter in `planfs_v1` block"),
            vec![],
        )
    })?;
    if !yaml_value.is_mapping() {
        return Err(ai_error(
            "INVALID_INPUT",
            "PLAN.md [LEGEND] must contain mapping under `planfs_v1`",
        ));
    }
    let plan_yaml: PlanFsPlanYaml = serde_yaml::from_str(legend).map_err(|err| {
        ai_error_with(
            "INVALID_INPUT",
            &format!("PLAN.md mapping schema invalid: {err}"),
            Some("Keep PLAN.md front matter with keys: plan_slug, title, objective, slices"),
            vec![],
        )
    })?;
    let header = plan_yaml.planfs_v1;
    validate_plan_front_matter(&header, strict)?;

    let sections = parse_sections(content, strict, limits)?;

    let refs = header
        .slices
        .into_iter()
        .map(|slice_ref| PlanFsSliceRef {
            id: slice_ref.id,
            title: slice_ref.title,
            file: slice_ref.file,
            status: slice_ref.status,
        })
        .collect::<Vec<_>>();

    if refs.is_empty() && strict {
        return Err(ai_error(
            "INVALID_INPUT",
            "PLAN.md slices is required in strict mode",
        ));
    }

    if refs.len() > limits.max_slices {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "PLAN.md slices exceeds max_slices budget: {} > {}",
                refs.len(),
                limits.max_slices
            ),
            Some("Split the plan into fewer slices or raise max_slices"),
            vec![],
        ));
    }

    let plan = PlanFsPlan {
        plan_slug: header.plan_slug,
        title: header.title,
        objective: header.objective,
        constraints: header.constraints,
        policy: header.policy,
        slices: refs,
        sections,
    };
    Ok((plan.clone(), plan.slices.clone()))
}

#[allow(dead_code)]
pub(crate) fn parse_plan_file(
    raw: &str,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<PlanFsPlan, Value> {
    let (plan, refs) = parse_plan_with_front_matter(raw, strict, limits)?;
    let mut plan = plan;
    plan.slices = refs;
    Ok(plan)
}

pub(crate) fn parse_slice_with_front_matter(
    raw: &str,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<PlanFsSlice, Value> {
    let (legend, content) = split_file(raw)?;
    let yaml: YamlValue = serde_yaml::from_str(legend).map_err(|err| {
        ai_error_with(
            "INVALID_INPUT",
            &format!("Slice-* [LEGEND] must be valid YAML: {err}"),
            Some("Fix `planfs_v1` block in slice file"),
            vec![],
        )
    })?;
    if !yaml.is_mapping() {
        return Err(ai_error(
            "INVALID_INPUT",
            "Slice file [LEGEND] must contain mapping under `planfs_v1`",
        ));
    }
    let yaml: PlanFsSliceYaml = serde_yaml::from_str(legend).map_err(|err| {
        ai_error_with(
            "INVALID_INPUT",
            &format!("Slice file mapping schema invalid: {err}"),
            Some("Slice file must include id/title/objective/budgets/dod/tasks"),
            vec![],
        )
    })?;
    let header = yaml.planfs_v1;
    let sections = parse_sections(content, strict, limits)?;

    validate_budgets(&header.budgets)?;
    validate_task_bundle(&header.tasks, strict, limits)?;

    let section_tasks = header.tasks;
    let tasks = section_tasks
        .into_iter()
        .enumerate()
        .map(|(idx, task)| parse_task(task, idx, strict, limits))
        .collect::<Result<Vec<_>, _>>()?;

    let dod = parse_dod(header.dod, strict, strict, limits)?;

    Ok(PlanFsSlice {
        id: header.id,
        title: header.title,
        objective: header.objective,
        status: header.status,
        budgets: header.budgets,
        dod,
        tasks,
        sections,
    })
}

#[allow(dead_code)]
pub(crate) fn parse_slice_file(
    raw: &str,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<PlanFsSlice, Value> {
    parse_slice_with_front_matter(raw, strict, limits)
}

pub(crate) fn looks_like_placeholder(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return true;
    }
    PLACEHOLDER_PATTERNS.iter().any(|pattern| {
        if pattern.starts_with('<') {
            value.starts_with(pattern)
        } else {
            value == *pattern
        }
    })
}

fn split_file(raw: &str) -> Result<(&str, &str), Value> {
    if raw.len() > 240_000 {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "planfs file exceeds safe read size",
            Some("Use smaller docs or raise max_file_bytes"),
            vec![],
        ));
    }

    let mut lines = raw.lines();
    let first = lines
        .next()
        .ok_or_else(|| ai_error("INVALID_INPUT", "empty planfs file"))?;
    if first.trim() != LEGEND_MARKER {
        return Err(ai_error(
            "INVALID_INPUT",
            "planfs file must begin with [LEGEND]",
        ));
    }

    let legend_pos = raw.find(LEGEND_MARKER).ok_or_else(|| {
        ai_error(
            "INVALID_INPUT",
            "could not locate [LEGEND] marker while parsing",
        )
    })?;
    let rest = &raw[(legend_pos + LEGEND_MARKER.len())..];
    let content_pos = rest
        .find(CONTENT_MARKER)
        .ok_or_else(|| ai_error("INVALID_INPUT", "planfs file must include [CONTENT] marker"))?;
    let first_offset = legend_pos + LEGEND_MARKER.len() + content_pos;
    if rest[(content_pos + CONTENT_MARKER.len())..].contains(CONTENT_MARKER) {
        return Err(ai_error(
            "INVALID_INPUT",
            "planfs file must include only one [CONTENT] marker",
        ));
    }

    let legend = &raw[(legend_pos + LEGEND_MARKER.len())..first_offset];
    let content = &raw[(first_offset + CONTENT_MARKER.len())..];
    Ok((legend.trim(), content.trim_start_matches('\n').trim_end()))
}

fn parse_sections(
    raw: &str,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<PlanFsSectionBundle, Value> {
    let mut buckets: BTreeMap<String, String> = BTreeMap::new();
    let mut current = String::new();
    let mut lines = Vec::<String>::new();

    for line in raw.lines() {
        if let Some(title_raw) = line.strip_prefix("## ") {
            if !current.is_empty() {
                buckets.insert(current.clone(), lines.join("\n"));
            }
            current = normalize_section_key(title_raw);
            lines.clear();
            continue;
        }
        if !current.is_empty() {
            lines.push(line.to_string());
        }
    }
    if !current.is_empty() {
        buckets.insert(current, lines.join("\n"));
    }

    let sections = PlanFsSectionBundle {
        goal: parse_goal_section(buckets.get("goal").map(String::as_str), strict, "goal")?,
        scope: parse_bullet_section(
            buckets.get("scope").map(String::as_str),
            strict,
            limits,
            "scope",
        )?,
        non_goals: parse_bullet_section(
            buckets.get("non-goals").map(String::as_str),
            strict,
            limits,
            "non-goals",
        )?,
        interfaces: parse_bullet_section(
            buckets.get("interfaces").map(String::as_str),
            strict,
            limits,
            "interfaces",
        )?,
        contracts: parse_bullet_section(
            buckets.get("contracts").map(String::as_str),
            strict,
            limits,
            "contracts",
        )?,
        tests: parse_bullet_section(
            buckets.get("tests").map(String::as_str),
            strict,
            limits,
            "tests",
        )?,
        proof: parse_bullet_section(
            buckets.get("proof").map(String::as_str),
            strict,
            limits,
            "proof",
        )?,
        rollback: parse_bullet_section(
            buckets.get("rollback").map(String::as_str),
            strict,
            limits,
            "rollback",
        )?,
        risks: parse_bullet_section(
            buckets.get("risks").map(String::as_str),
            strict,
            limits,
            "risks",
        )?,
    };

    if strict {
        for key in PLAN_REQUIRED_SECTIONS {
            if !buckets.contains_key(*key) {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("missing required section: {key}"),
                    Some("Include sections in [CONTENT] using `##` headings"),
                    vec![],
                ));
            }
        }
    }
    Ok(sections)
}

fn parse_goal_section(raw: Option<&str>, strict: bool, field: &str) -> Result<String, Value> {
    let value = raw.unwrap_or("").trim().to_string();
    if strict && value.trim().is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field} must not be empty"),
            Some("Add a concise goal statement"),
            vec![],
        ));
    }
    if strict && looks_like_placeholder(&value) {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field} looks like placeholder"),
            Some("Replace placeholder with concrete non-empty statement"),
            vec![],
        ));
    }
    Ok(value)
}

fn parse_bullet_section(
    raw: Option<&str>,
    strict: bool,
    limits: &PlanFsReadLimits,
    field: &str,
) -> Result<Vec<String>, Value> {
    let lines = raw.unwrap_or("");
    let mut out = Vec::<String>::new();
    for line in lines.lines() {
        let normalized = normalize_bullet_line(line);
        if normalized.is_empty() {
            continue;
        }
        out.push(normalized);
    }

    if strict && out.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field} must have at least 1 list item"),
            Some("Add concrete criteria or blockers to this section"),
            vec![],
        ));
    }

    if out.len() > limits.max_items_per_list {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field} list too large"),
            Some("Trim section or increase max_items_per_list budget"),
            vec![],
        ));
    }

    if strict {
        for item in &out {
            if looks_like_placeholder(item) {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("{field} item looks like placeholder: {item}"),
                    Some("Replace placeholders with concrete items"),
                    vec![],
                ));
            }
        }
    }

    Ok(out)
}

fn parse_list(raw: &[String], field: &str, strict: bool) -> Result<Vec<String>, Value> {
    let mut out = Vec::<String>::new();
    for item in raw {
        let item = item.trim().to_string();
        if item.is_empty() {
            continue;
        }
        out.push(item);
    }
    if strict && out.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field} cannot be empty"),
            Some("Provide at least one entry"),
            vec![],
        ));
    }
    if strict {
        for item in &out {
            if looks_like_placeholder(item) {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("{field} contains placeholder"),
                    Some("Replace placeholders with concrete concrete text"),
                    vec![],
                ));
            }
        }
    }
    Ok(out)
}

fn parse_dod(
    raw: PlanFsDod,
    strict: bool,
    _section: bool,
    _limits: &PlanFsReadLimits,
) -> Result<PlanFsDod, Value> {
    let mut dod = raw;
    dod.success_criteria = parse_list(&dod.success_criteria, "slice.dod.success_criteria", strict)?;
    dod.tests = parse_list(&dod.tests, "slice.dod.tests", strict)?;
    dod.blockers = parse_list(&dod.blockers, "slice.dod.blockers", strict)?;
    dod.rollback = parse_list(&dod.rollback, "slice.dod.rollback", strict)?;
    Ok(dod)
}

fn parse_task(
    raw: PlanFsTask,
    idx: usize,
    strict: bool,
    limits: &PlanFsReadLimits,
) -> Result<PlanFsTask, Value> {
    let title_field = format!("tasks[{idx}].title");
    let steps_len = raw.steps.len();
    if strict && !(MIN_STEP_LIST_LEN..=MAX_STEP_LIST_LEN).contains(&steps_len) {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "{title_field}: steps must be between {MIN_STEP_LIST_LEN} and {MAX_STEP_LIST_LEN}"
            ),
            Some("Split/merge steps to keep 3..10 items"),
            vec![],
        ));
    }

    let mut out = raw;
    out.title = out.title.trim().to_string();
    if out.title.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{title_field}: title must not be empty"),
            Some("Provide explicit task title"),
            vec![],
        ));
    }
    out.success_criteria = parse_list(
        &out.success_criteria,
        &format!("tasks[{idx}].success_criteria"),
        strict,
    )?;
    out.tests = parse_list(&out.tests, &format!("tasks[{idx}].tests"), strict)?;
    out.blockers = parse_list(&out.blockers, &format!("tasks[{idx}].blockers"), strict)?;
    out.rollback = parse_list(&out.rollback, &format!("tasks[{idx}].rollback"), strict)?;
    if strict
        && (out.success_criteria.len() > limits.max_items_per_list
            || out.tests.len() > limits.max_items_per_list
            || out.blockers.len() > limits.max_items_per_list
            || out.rollback.len() > limits.max_items_per_list)
    {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{title_field}: list exceeds max_items_per_list"),
            Some("Trim section or raise max_items_per_list"),
            vec![],
        ));
    }

    let mut steps = Vec::with_capacity(out.steps.len());
    for (step_idx, step) in out.steps.into_iter().enumerate() {
        steps.push(parse_step(step, idx, step_idx, strict, limits)?);
    }
    out.steps = steps;
    if strict && out.steps.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{title_field}: steps cannot be empty"),
            Some("Add at least one concrete step"),
            vec![],
        ));
    }
    Ok(out)
}

fn parse_step(
    raw: PlanFsStep,
    task_idx: usize,
    step_idx: usize,
    strict: bool,
    _limits: &PlanFsReadLimits,
) -> Result<PlanFsStep, Value> {
    let field = format!("tasks[{task_idx}].steps[{step_idx}]");
    let mut out = raw;
    out.title = out.title.trim().to_string();
    if out.title.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("{field}.title must not be empty"),
            Some("Provide explicit step title"),
            vec![],
        ));
    }
    out.success_criteria = parse_list(
        &out.success_criteria,
        &format!("{field}.success_criteria"),
        strict,
    )?;
    out.tests = parse_list(&out.tests, &format!("{field}.tests"), strict)?;
    out.blockers = parse_list(&out.blockers, &format!("{field}.blockers"), strict)?;
    out.rollback = parse_list(&out.rollback, &format!("{field}.rollback"), strict)?;
    Ok(out)
}

fn validate_budgets(budgets: &PlanFsBudgets) -> Result<(), Value> {
    if budgets.max_files == 0 || budgets.max_diff_lines == 0 || budgets.max_context_refs == 0 {
        return Err(ai_error(
            "INVALID_INPUT",
            "slice budgets must have all values > 0",
        ));
    }
    Ok(())
}

fn validate_task_bundle(
    tasks: &[PlanFsTask],
    strict: bool,
    _limits: &PlanFsReadLimits,
) -> Result<(), Value> {
    if strict && (tasks.len() < MIN_SLICE_TASKS || tasks.len() > MAX_SLICE_TASKS) {
        return Err(ai_error(
            "INVALID_INPUT",
            "slice tasks count must be in range 3..10 in strict mode",
        ));
    }
    if tasks.is_empty() {
        return Err(ai_error("INVALID_INPUT", "slice tasks must not be empty"));
    }
    if strict {
        let mut seen_task_titles = std::collections::HashSet::<String>::new();
        for task in tasks {
            let key = task.title.to_ascii_lowercase();
            if !seen_task_titles.insert(key) {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    "slice tasks contain duplicates",
                    Some("Make task titles unique"),
                    vec![],
                ));
            }
        }
    }
    Ok(())
}

fn validate_plan_front_matter(header: &PlanFsPlanYamlHeader, strict: bool) -> Result<(), Value> {
    if header.plan_slug.trim().is_empty() {
        return Err(ai_error("INVALID_INPUT", "planfs_v1.plan_slug is required"));
    }
    if header.title.trim().is_empty() {
        return Err(ai_error("INVALID_INPUT", "planfs_v1.title is required"));
    }
    if header.objective.trim().is_empty() {
        return Err(ai_error("INVALID_INPUT", "planfs_v1.objective is required"));
    }
    if strict {
        for (idx, slice_ref) in header.slices.iter().enumerate() {
            if slice_ref.id.trim().is_empty() {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("planfs_v1.slices[{idx}].id is required"),
                    Some("Provide stable slice ids: `SLICE-1`, `SLICE-2`, ..."),
                    vec![],
                ));
            }
            if slice_ref.file.trim().is_empty() {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("planfs_v1.slices[{idx}].file is required"),
                    Some("Set `file` to Slice-*.md for slice file"),
                    vec![],
                ));
            }
        }
        if header.slices.is_empty() {
            return Err(ai_error_with(
                "INVALID_INPUT",
                "planfs_v1.slices must not be empty",
                Some("Add at least one Slice-* file and reference it here"),
                vec![],
            ));
        }
    }
    Ok(())
}

fn normalize_section_key(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace("–", "-")
        .replace(' ', "-")
}

fn normalize_bullet_line(raw: &str) -> String {
    let mut item = raw.trim();
    if let Some(rest) = item.strip_prefix("- [ ]") {
        item = rest;
    } else if let Some(rest) = item.strip_prefix("- [x]") {
        item = rest;
    }
    let item = item.trim_start_matches("- ");
    let item = item.trim_start_matches("* ");
    item.trim().trim_matches('"').trim_matches('`').to_string()
}
