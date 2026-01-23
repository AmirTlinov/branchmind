#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResumeSuperView {
    Full,
    FocusOnly,
    Smart,
    Explore,
    Audit,
}

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperArgs {
    pub(super) workspace: WorkspaceId,
    pub(super) view: ResumeSuperView,
    pub(super) max_chars: Option<usize>,
    pub(super) agent_id: Option<String>,
    pub(super) events_limit: usize,
    pub(super) decisions_limit: usize,
    pub(super) evidence_limit: usize,
    pub(super) blockers_limit: usize,
    pub(super) notes_limit: usize,
    pub(super) trace_limit: usize,
    pub(super) cards_limit: usize,
    pub(super) engine_signals_limit: usize,
    pub(super) engine_actions_limit: usize,
    pub(super) notes_cursor: Option<i64>,
    pub(super) trace_cursor: Option<i64>,
    pub(super) cards_cursor: Option<i64>,
    pub(super) graph_diff_cursor: Option<i64>,
    pub(super) graph_diff_limit: usize,
    pub(super) include_graph_diff: bool,
    pub(super) read_only: bool,
    pub(super) explicit_target: Option<String>,
}

pub(super) fn parse_resume_super_args(
    args_obj: &serde_json::Map<String, Value>,
) -> Result<ResumeSuperArgs, Value> {
    let workspace = require_workspace(args_obj)?;
    let agent_id = optional_agent_id(args_obj, "agent_id")?;
    let context_budget = optional_usize(args_obj, "context_budget")?;
    let view = optional_string(args_obj, "view")?
        .unwrap_or_else(|| {
            if context_budget.is_some() {
                "smart".to_string()
            } else {
                "full".to_string()
            }
        })
        .to_lowercase();
    let view = match view.as_str() {
        "full" => ResumeSuperView::Full,
        "focus_only" => ResumeSuperView::FocusOnly,
        "smart" => ResumeSuperView::Smart,
        "explore" => ResumeSuperView::Explore,
        "audit" => ResumeSuperView::Audit,
        _ => {
            return Err(ai_error_with(
                "INVALID_INPUT",
                "Unsupported view",
                Some("Supported: full, smart, explore, audit, focus_only"),
                vec![],
            ));
        }
    };
    let max_chars = optional_usize(args_obj, "max_chars")?;
    let max_chars = match (context_budget, max_chars) {
        (None, v) => v,
        (Some(budget), None) => Some(budget),
        (Some(budget), Some(explicit)) => Some(explicit.min(budget)),
    };

    let defaults = defaults_for_view(view, max_chars);

    let events_limit = optional_usize(args_obj, "events_limit")?.unwrap_or(defaults.events_limit);
    let decisions_limit =
        optional_usize(args_obj, "decisions_limit")?.unwrap_or(defaults.decisions_limit);
    let evidence_limit =
        optional_usize(args_obj, "evidence_limit")?.unwrap_or(defaults.evidence_limit);
    let blockers_limit =
        optional_usize(args_obj, "blockers_limit")?.unwrap_or(defaults.blockers_limit);
    let notes_limit = optional_usize(args_obj, "notes_limit")?.unwrap_or(defaults.notes_limit);
    let trace_limit = optional_usize(args_obj, "trace_limit")?.unwrap_or(defaults.trace_limit);
    let cards_limit = optional_usize(args_obj, "cards_limit")?.unwrap_or(defaults.cards_limit);

    let engine_signals_limit =
        optional_usize(args_obj, "engine_signals_limit")?.unwrap_or(defaults.engine_signals_limit);
    let engine_actions_limit =
        optional_usize(args_obj, "engine_actions_limit")?.unwrap_or(defaults.engine_actions_limit);

    let notes_cursor = optional_i64(args_obj, "notes_cursor")?;
    let trace_cursor = optional_i64(args_obj, "trace_cursor")?;
    let cards_cursor = optional_i64(args_obj, "cards_cursor")?;

    let graph_diff_cursor = optional_i64(args_obj, "graph_diff_cursor")?;
    let graph_diff_limit = optional_usize(args_obj, "graph_diff_limit")?;
    let graph_diff = args_obj
        .get("graph_diff")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let include_graph_diff =
        graph_diff || graph_diff_limit.is_some() || graph_diff_cursor.is_some();
    let graph_diff_limit = graph_diff_limit.unwrap_or(50).max(1);

    let read_only = args_obj
        .get("read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let explicit_target = args_obj
        .get("task")
        .and_then(|v| v.as_str())
        .or_else(|| args_obj.get("plan").and_then(|v| v.as_str()))
        .map(|v| v.to_string());

    Ok(ResumeSuperArgs {
        workspace,
        view,
        max_chars,
        agent_id,
        events_limit,
        decisions_limit,
        evidence_limit,
        blockers_limit,
        notes_limit,
        trace_limit,
        cards_limit,
        engine_signals_limit,
        engine_actions_limit,
        notes_cursor,
        trace_cursor,
        cards_cursor,
        graph_diff_cursor,
        graph_diff_limit,
        include_graph_diff,
        read_only,
        explicit_target,
    })
}

#[derive(Clone, Copy, Debug)]
struct ResumeSuperDefaults {
    events_limit: usize,
    decisions_limit: usize,
    evidence_limit: usize,
    blockers_limit: usize,
    notes_limit: usize,
    trace_limit: usize,
    cards_limit: usize,
    engine_signals_limit: usize,
    engine_actions_limit: usize,
}

fn defaults_for_view(view: ResumeSuperView, max_chars: Option<usize>) -> ResumeSuperDefaults {
    // Deterministic heuristics: these defaults are only used when the caller didn't explicitly
    // set a per-section limit. The global `max_chars` still applies as a hard output cap.
    //
    // Goal: reduce limit-juggling when `context_budget` is used, while keeping compatibility
    // for legacy callers on the default `view="full"`.
    match view {
        ResumeSuperView::Full => ResumeSuperDefaults {
            events_limit: 20,
            decisions_limit: 5,
            evidence_limit: 5,
            blockers_limit: 5,
            notes_limit: 10,
            trace_limit: 20,
            cards_limit: 20,
            engine_signals_limit: 6,
            engine_actions_limit: 6,
        },
        ResumeSuperView::FocusOnly => ResumeSuperDefaults {
            // Focus-only is intentionally minimal; the view shaper will also aggressively
            // compress the output after the budget pass.
            events_limit: 12,
            decisions_limit: 3,
            evidence_limit: 3,
            blockers_limit: 3,
            notes_limit: 3,
            trace_limit: 8,
            cards_limit: 40,
            engine_signals_limit: 6,
            // One best action + one backup to avoid "action sprawl" in focus mode.
            engine_actions_limit: 2,
        },
        ResumeSuperView::Smart | ResumeSuperView::Explore => {
            let tier = budget_tier(max_chars.unwrap_or(12_000));
            match tier {
                BudgetTier::Tiny => ResumeSuperDefaults {
                    events_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    decisions_limit: 3,
                    evidence_limit: 3,
                    blockers_limit: 3,
                    notes_limit: if view == ResumeSuperView::Explore {
                        6
                    } else {
                        4
                    },
                    trace_limit: if view == ResumeSuperView::Explore {
                        14
                    } else {
                        10
                    },
                    cards_limit: if view == ResumeSuperView::Explore {
                        26
                    } else {
                        18
                    },
                    engine_signals_limit: if view == ResumeSuperView::Explore {
                        6
                    } else {
                        5
                    },
                    engine_actions_limit: 2,
                },
                BudgetTier::Small => ResumeSuperDefaults {
                    events_limit: if view == ResumeSuperView::Explore {
                        12
                    } else {
                        10
                    },
                    decisions_limit: if view == ResumeSuperView::Explore {
                        5
                    } else {
                        4
                    },
                    evidence_limit: if view == ResumeSuperView::Explore {
                        5
                    } else {
                        4
                    },
                    blockers_limit: if view == ResumeSuperView::Explore {
                        5
                    } else {
                        4
                    },
                    notes_limit: if view == ResumeSuperView::Explore {
                        12
                    } else {
                        8
                    },
                    trace_limit: if view == ResumeSuperView::Explore {
                        22
                    } else {
                        16
                    },
                    cards_limit: if view == ResumeSuperView::Explore {
                        40
                    } else {
                        26
                    },
                    engine_signals_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    engine_actions_limit: 2,
                },
                BudgetTier::Medium => ResumeSuperDefaults {
                    events_limit: if view == ResumeSuperView::Explore {
                        16
                    } else {
                        14
                    },
                    decisions_limit: if view == ResumeSuperView::Explore {
                        6
                    } else {
                        5
                    },
                    evidence_limit: if view == ResumeSuperView::Explore {
                        6
                    } else {
                        5
                    },
                    blockers_limit: if view == ResumeSuperView::Explore {
                        6
                    } else {
                        5
                    },
                    notes_limit: if view == ResumeSuperView::Explore {
                        16
                    } else {
                        12
                    },
                    trace_limit: if view == ResumeSuperView::Explore {
                        34
                    } else {
                        24
                    },
                    cards_limit: if view == ResumeSuperView::Explore {
                        70
                    } else {
                        40
                    },
                    engine_signals_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    engine_actions_limit: 2,
                },
                BudgetTier::Large => ResumeSuperDefaults {
                    events_limit: if view == ResumeSuperView::Explore {
                        24
                    } else {
                        20
                    },
                    decisions_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    evidence_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    blockers_limit: if view == ResumeSuperView::Explore {
                        8
                    } else {
                        6
                    },
                    notes_limit: if view == ResumeSuperView::Explore {
                        22
                    } else {
                        12
                    },
                    trace_limit: if view == ResumeSuperView::Explore {
                        48
                    } else {
                        30
                    },
                    cards_limit: if view == ResumeSuperView::Explore {
                        100
                    } else {
                        60
                    },
                    engine_signals_limit: if view == ResumeSuperView::Explore {
                        10
                    } else {
                        8
                    },
                    engine_actions_limit: 2,
                },
            }
        }
        ResumeSuperView::Audit => ResumeSuperDefaults {
            // Audit is relevance-first but cross-lane; keep it cold-archive by default to avoid noise.
            // Callers can always raise per-section limits explicitly.
            events_limit: 14,
            decisions_limit: 6,
            evidence_limit: 6,
            blockers_limit: 6,
            notes_limit: 12,
            trace_limit: 24,
            cards_limit: 60,
            engine_signals_limit: 8,
            engine_actions_limit: 2,
        },
    }
}

#[derive(Clone, Copy, Debug)]
enum BudgetTier {
    Tiny,
    Small,
    Medium,
    Large,
}

fn budget_tier(max_chars: usize) -> BudgetTier {
    match max_chars {
        0..=2_000 => BudgetTier::Tiny,
        2_001..=6_000 => BudgetTier::Small,
        6_001..=12_000 => BudgetTier::Medium,
        _ => BudgetTier::Large,
    }
}
