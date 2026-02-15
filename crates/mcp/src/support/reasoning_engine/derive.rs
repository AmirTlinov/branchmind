#![forbid(unsafe_code)]

use super::*;

pub(crate) fn derive_reasoning_engine(
    scope: EngineScope<'_>,
    cards: &[Value],
    edges: &[Value],
    trace_entries: &[Value],
    limits: EngineLimits,
) -> Option<Value> {
    if limits.signals_limit == 0 && limits.actions_limit == 0 {
        return None;
    }

    let mut signals: Vec<EngineSignal> = Vec::new();
    let mut actions: Vec<EngineAction> = Vec::new();
    let reference_ts_ms = max_ts_ms(cards, trace_entries);

    // Build id -> card lookup (small; deterministic via BTreeMap key ordering).
    let mut by_id = std::collections::BTreeMap::<String, &Value>::new();
    for card in cards {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        by_id.insert(id.to_string(), card);
    }

    // Build incoming adjacency for supports/blocks.
    let mut incoming_supports = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut incoming_blocks = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut outgoing_supports = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut outgoing_blocks = std::collections::BTreeMap::<String, Vec<String>>::new();
    for edge in edges {
        let Some((from, rel, to)) = edge_triplet(edge) else {
            continue;
        };
        if !by_id.contains_key(from) || !by_id.contains_key(to) {
            continue;
        }
        match rel {
            "supports" => {
                incoming_supports
                    .entry(to.to_string())
                    .or_default()
                    .push(from.to_string());
                outgoing_supports
                    .entry(from.to_string())
                    .or_default()
                    .push(to.to_string());
            }
            "blocks" => {
                incoming_blocks
                    .entry(to.to_string())
                    .or_default()
                    .push(from.to_string());
                outgoing_blocks
                    .entry(from.to_string())
                    .or_default()
                    .push(to.to_string());
            }
            _ => {}
        }
    }
    for list in incoming_supports.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in incoming_blocks.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in outgoing_supports.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in outgoing_blocks.values_mut() {
        list.sort();
        list.dedup();
    }

    // ===== BM2: Evidence strength scoring (deterministic, slice-only) =====
    let mut evidence_scores = std::collections::BTreeMap::<String, u8>::new();
    for (id, card) in by_id.iter() {
        if card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
            continue;
        }
        let score = evidence_strength_score(card, &outgoing_supports, &outgoing_blocks, &by_id);
        evidence_scores.insert(id.to_string(), score);
    }

    // ===== Draft hygiene: draft decisions should be promoted into canon =====
    // Motivation: drafts are intentionally low-visibility, but decisions are knowledge anchors and
    // should not silently remain stuck as `v:draft` forever.
    //
    // Deterministic: derived from the returned slice only (no extra store reads).
    let recent_window_ms = 14i64.saturating_mul(ms_per_day());
    let mut lane_decisions: Vec<&Value> = by_id
        .values()
        .filter(|card| card.get("type").and_then(|v| v.as_str()) == Some("decision"))
        .filter(|card| card_is_draft_like(card))
        .copied()
        .collect();
    lane_decisions.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for decision in lane_decisions.iter().take(8) {
        let Some(decision_id) = decision.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if decision_id.trim().starts_with("CARD-PUB-") {
            continue;
        }
        let decision_ts_ms = decision
            .get("last_ts_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let is_pinned = card_has_tag(decision, PIN_TAG);
        let is_recent = reference_ts_ms == 0
            || decision_ts_ms >= reference_ts_ms.saturating_sub(recent_window_ms);
        if !is_pinned && !is_recent {
            continue;
        }

        // Best-effort slice-only check: if the deterministic published id is present in this slice,
        // do not emit a redundant publish suggestion.
        let published_id = format!("CARD-PUB-{}", decision_id.trim());
        if by_id.contains_key(published_id.as_str()) {
            continue;
        }

        let label = shorten(&card_label(decision), 64);
        signals.push(signal_at(
            "BM_LANE_DECISION_NOT_PUBLISHED",
            "warning",
            format!("Decision is draft-scoped (v:draft) and not promoted to canon: {label}"),
            vec![ref_card(decision_id)],
            decision_ts_ms,
        ));

        actions.push(action_at(
            "publish_decision",
            "medium",
            format!("Promote decision to canon (pinned): {label}"),
            Some(
                "Draft hygiene: promote decisions so they become stable resume anchors across sessions."
                    .to_string(),
            ),
            vec![ref_card(decision_id)],
            vec![suggest_call(
                "think_publish",
                "Promote this decision into canon (deterministic published copy).",
                "medium",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card_id": decision_id,
                    "pin": true
                }),
            )],
            decision_ts_ms,
        ));
    }

    // ===== BM4: Blind spot detection (hypothesis without tests/evidence) =====
    let mut hypotheses: Vec<&Value> = by_id
        .values()
        .filter(|card| card.get("type").and_then(|v| v.as_str()) == Some("hypothesis"))
        // Treat hypotheses as active unless explicitly closed. This prevents bypassing
        // discipline checks via status drift (e.g. "accepted", "done").
        .filter(|card| card_status_is_active_for_discipline(card))
        .copied()
        .collect();
    hypotheses.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for hypo in hypotheses.iter().take(12) {
        let Some(hypo_id) = hypo.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let hypo_ts_ms = hypo.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let supporters = incoming_supports.get(hypo_id);
        let mut supporting_tests: Vec<&Value> = Vec::new();
        let mut direct_evidence = false;

        if let Some(ids) = supporters {
            for from_id in ids {
                let Some(from_card) = by_id.get(from_id).copied() else {
                    continue;
                };
                match from_card.get("type").and_then(|v| v.as_str()) {
                    Some("test") => supporting_tests.push(from_card),
                    Some("evidence") => direct_evidence = true,
                    _ => {}
                }
            }
        }

        let mut indirect_evidence = false;
        for test in &supporting_tests {
            let Some(test_id) = test.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(ids) = incoming_supports.get(test_id) {
                for from_id in ids {
                    let Some(from_card) = by_id.get(from_id).copied() else {
                        continue;
                    };
                    if from_card.get("type").and_then(|v| v.as_str()) == Some("evidence") {
                        indirect_evidence = true;
                        break;
                    }
                }
            }
            if indirect_evidence {
                break;
            }
        }

        if supporting_tests.is_empty() {
            let label = shorten(&card_label(hypo), 64);
            signals.push(signal_at(
                "BM4_HYPOTHESIS_NO_TEST",
                "high",
                format!("Hypothesis has no linked tests: {label}"),
                vec![ref_card(hypo_id)],
                hypo_ts_ms,
            ));
            let calls = vec![suggest_call(
                "think_card",
                "Create a test stub that supports this hypothesis (fill command later).",
                "high",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "test",
                        "title": format!("Test: {label}"),
                        "text": "Define the smallest runnable check for this hypothesis.",
                        "status": "open",
                        "tags": ["bm4"]
                    },
                    "supports": [hypo_id]
                }),
            )];
            actions.push(action_at(
                "add_test_stub",
                "high",
                format!("Add a test for: {label}"),
                Some("BM4: no linked tests found in current slice.".to_string()),
                vec![ref_card(hypo_id)],
                calls,
                hypo_ts_ms,
            ));
        } else if !direct_evidence && !indirect_evidence {
            let label = shorten(&card_label(hypo), 64);
            signals.push(signal_at(
                "BM4_HYPOTHESIS_NO_EVIDENCE",
                "warning",
                format!("Hypothesis has tests but no linked evidence (in slice): {label}"),
                vec![ref_card(hypo_id)],
                hypo_ts_ms,
            ));
        }
    }

    // ===== BM1: Contradiction detection (supports + blocks on same target) =====
    // Deterministic heuristic: if a card has both incoming supports and incoming blocks edges
    // (within the returned slice), surface it as a contradiction that needs a disambiguating test
    // or an explicit decision.
    let mut contradiction_targets: Vec<&Value> = by_id
        .values()
        .filter(|card| {
            let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
            matches!(ty, "hypothesis" | "test" | "decision")
        })
        .filter(|card| {
            card.get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("open")
                == "open"
        })
        .copied()
        .collect();
    contradiction_targets.sort_by(|a, b| {
        let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        b_ts.cmp(&a_ts).then_with(|| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        })
    });

    for target in contradiction_targets.iter().take(10) {
        let Some(target_id) = target.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let supports = incoming_supports
            .get(target_id)
            .cloned()
            .unwrap_or_default();
        let blocks = incoming_blocks.get(target_id).cloned().unwrap_or_default();
        if supports.is_empty() || blocks.is_empty() {
            continue;
        }

        let target_ts_ms = target
            .get("last_ts_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let label = shorten(&card_label(target), 64);

        let mut refs = vec![ref_card(target_id)];
        refs.extend(refs_from_ids(&supports, 2));
        refs.extend(refs_from_ids(&blocks, 2));

        signals.push(signal_at(
            "BM1_CONTRADICTION_SUPPORTS_BLOCKS",
            "high",
            format!("Contradiction detected (supports vs blocks) for: {label}"),
            refs.clone(),
            target_ts_ms,
        ));

        let calls = vec![
            suggest_call(
                "think_playbook",
                "Load a deterministic contradiction-resolution playbook.",
                "medium",
                json!({ "workspace": scope.workspace, "name": "contradiction" }),
            ),
            suggest_call(
                "think_card",
                "Write a focused question that forces a decisive test or decision.",
                "high",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "question",
                        "title": format!("Resolve contradiction: {label}"),
                        "text": "List strongest evidence on both sides, then define the smallest decisive test.",
                        "status": "open",
                        "tags": ["bm1", "contradiction"],
                        "meta": { "about": { "kind": "card", "id": target_id } }
                    }
                }),
            ),
        ];
        actions.push(action_at(
            "resolve_contradiction",
            "high",
            format!("Resolve contradiction: {label}"),
            Some("BM1: both supports and blocks edges exist in the current slice.".to_string()),
            refs,
            calls,
            target_ts_ms,
        ));
    }

    // ===== BM2: Evidence strength (weak pinned evidence is actionable debt) =====
    // Keep output intentionally small: at most 2 weak evidence warnings.
    let mut pinned_decision_ids = std::collections::BTreeSet::<String>::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("decision") {
            continue;
        }
        if card_has_tag(card, PIN_TAG)
            && let Some(id) = card.get("id").and_then(|v| v.as_str())
        {
            pinned_decision_ids.insert(id.to_string());
        }
    }

    let mut weak_evidence: Vec<(&Value, u8)> = Vec::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let score = evidence_scores.get(id).copied().unwrap_or(0);
        if score >= 60 {
            continue;
        }

        let mut important = card_has_tag(card, PIN_TAG);
        if !important {
            if let Some(targets) = outgoing_supports.get(id) {
                important |= targets.iter().any(|t| pinned_decision_ids.contains(t));
            }
            if let Some(targets) = outgoing_blocks.get(id) {
                important |= targets.iter().any(|t| pinned_decision_ids.contains(t));
            }
        }
        if !important {
            continue;
        }

        weak_evidence.push((card, score));
    }
    weak_evidence.sort_by(|(a, ascore), (b, bscore)| {
        ascore.cmp(bscore).then_with(|| {
            let a_ts = a.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let b_ts = b.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            b_ts.cmp(&a_ts).then_with(|| {
                let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                a_id.cmp(b_id)
            })
        })
    });

    for (card, score) in weak_evidence.into_iter().take(2) {
        let Some(evidence_id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let receipts = evidence_receipts(card);
        let mut missing = Vec::<&str>::new();
        if !receipts.cmd {
            missing.push("CMD");
        }
        if !receipts.link {
            missing.push("LINK");
        }
        let missing = if missing.is_empty() {
            "receipts".to_string()
        } else {
            missing.join("+")
        };
        let label = shorten(&card_label(card), 64);
        signals.push(signal_at(
            "BM2_EVIDENCE_WEAK",
            "warning",
            format!("Evidence is weak (score {score}/100; missing {missing}): {label}"),
            vec![ref_card(evidence_id)],
            ts_ms,
        ));
    }

    // ===== BM3: Confidence propagation (what is actually proven?) =====
    // Deterministic, slice-only, shallow depth to avoid cycles.
    let mut memo = std::collections::BTreeMap::<String, f64>::new();
    let mut stack = std::collections::BTreeSet::<String>::new();

    let confidence_ctx = ConfidenceContext {
        by_id: &by_id,
        incoming_supports: &incoming_supports,
        incoming_blocks: &incoming_blocks,
        evidence_scores: &evidence_scores,
    };

    #[derive(Clone, Debug)]
    struct ConfidenceCandidate<'a> {
        id: &'a str,
        card: &'a Value,
        confidence: f64,
    }

    let mut pinned_decisions: Vec<ConfidenceCandidate<'_>> = Vec::new();
    let mut open_hypotheses: Vec<ConfidenceCandidate<'_>> = Vec::new();

    for card in by_id.values() {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ty {
            "decision" => {
                if !card_has_tag(card, PIN_TAG) {
                    continue;
                }
                let c = confidence_for_id(id, 3, &confidence_ctx, &mut memo, &mut stack);
                pinned_decisions.push(ConfidenceCandidate {
                    id,
                    card,
                    confidence: c,
                });
            }
            "hypothesis" => {
                if card
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("open")
                    != "open"
                {
                    continue;
                }
                let c = confidence_for_id(id, 3, &confidence_ctx, &mut memo, &mut stack);
                open_hypotheses.push(ConfidenceCandidate {
                    id,
                    card,
                    confidence: c,
                });
            }
            _ => {}
        }
    }

    pinned_decisions.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_ts = a
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let b_ts = b
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                b_ts.cmp(&a_ts).then_with(|| a.id.cmp(b.id))
            })
    });
    open_hypotheses.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_ts = a
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let b_ts = b
                    .card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                b_ts.cmp(&a_ts).then_with(|| a.id.cmp(b.id))
            })
    });

    if let Some(worst) = pinned_decisions.first() {
        let threshold = 0.55;
        if worst.confidence <= threshold {
            let ts_ms = worst
                .card
                .get("last_ts_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let supports = incoming_supports
                .get(worst.id)
                .map(|v| v.len())
                .unwrap_or(0);
            let blocks = incoming_blocks.get(worst.id).map(|v| v.len()).unwrap_or(0);
            let label = shorten(&card_label(worst.card), 64);
            signals.push(signal_at(
                "BM3_DECISION_LOW_CONFIDENCE",
                "warning",
                format!(
                    "Low confidence for pinned decision (~{:.2}); supports={} blocks={}: {label}",
                    worst.confidence, supports, blocks
                ),
                vec![ref_card(worst.id)],
                ts_ms,
            ));

            actions.push(action_at(
                "use_playbook",
                "medium",
                format!("Design a decisive experiment for: {label}"),
                Some(
                    "BM9: low-confidence anchors benefit from a single decisive experiment."
                        .to_string(),
                ),
                vec![ref_card(worst.id)],
                vec![suggest_call(
                    "think_playbook",
                    "Get a deterministic experiment playbook skeleton.",
                    "medium",
                    json!({ "workspace": scope.workspace, "name": "experiment" }),
                )],
                ts_ms,
            ));
        }
    } else if let Some(worst) = open_hypotheses.first() {
        let threshold = 0.45;
        if worst.confidence <= threshold {
            let ts_ms = worst
                .card
                .get("last_ts_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let supports = incoming_supports
                .get(worst.id)
                .map(|v| v.len())
                .unwrap_or(0);
            let blocks = incoming_blocks.get(worst.id).map(|v| v.len()).unwrap_or(0);
            let label = shorten(&card_label(worst.card), 64);
            signals.push(signal_at(
                "BM3_HYPOTHESIS_LOW_CONFIDENCE",
                "info",
                format!(
                    "Low confidence for hypothesis (~{:.2}); supports={} blocks={}: {label}",
                    worst.confidence, supports, blocks
                ),
                vec![ref_card(worst.id)],
                ts_ms,
            ));
        }
    }

    // ===== BM6: Assumption surfacing (cascade when assumptions change) =====
    // Heuristic: treat cards tagged `assumption` as first-class assumptions.
    // When a non-open assumption still supports active cards (open/pinned), surface it.
    #[derive(Clone, Debug)]
    struct AssumptionIssue {
        id: String,
        title: String,
        status: String,
        ts_ms: i64,
        impacted: Vec<String>,
    }

    let mut assumption_issues = Vec::<AssumptionIssue>::new();
    for card in by_id.values() {
        let tags = card_tags_lower(card);
        if !tags.iter().any(|t| t == "assumption") {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let status = card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            .trim()
            .to_string();
        if status.eq_ignore_ascii_case("open") {
            continue;
        }

        let mut impacted = Vec::<String>::new();
        if let Some(targets) = outgoing_supports.get(id) {
            for to in targets {
                let Some(target) = by_id.get(to).copied() else {
                    continue;
                };
                let ty = target.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if !matches!(ty, "decision" | "hypothesis") {
                    continue;
                }
                let active = card_has_tag(target, PIN_TAG)
                    || target
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("open")
                        .eq_ignore_ascii_case("open");
                if active {
                    impacted.push(to.to_string());
                }
            }
        }
        impacted.sort();
        impacted.dedup();
        if impacted.is_empty() {
            continue;
        }

        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let title = shorten(&card_label(card), 64);
        assumption_issues.push(AssumptionIssue {
            id: id.to_string(),
            title,
            status,
            ts_ms,
            impacted,
        });
    }
    assumption_issues.sort_by(|a, b| {
        b.ts_ms
            .cmp(&a.ts_ms)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.title.cmp(&b.title))
    });

    if let Some(issue) = assumption_issues.into_iter().next() {
        let refs = {
            let mut refs = vec![ref_card(issue.id.as_str())];
            refs.extend(refs_from_ids(&issue.impacted, 4));
            refs
        };
        signals.push(signal_at(
            "BM6_ASSUMPTION_NOT_OPEN_BUT_USED",
            "warning",
            format!(
                "Assumption is not open (status={}) but still supports {} active cards: {}",
                issue.status,
                issue.impacted.len(),
                issue.title
            ),
            refs.clone(),
            issue.ts_ms,
        ));
        actions.push(action_at(
            "recheck_assumption",
            "medium",
            format!("Recheck assumption cascade: {}", issue.title),
            Some(
                "BM6: when an assumption changes, dependent cards must be re-evaluated."
                    .to_string(),
            ),
            refs,
            vec![suggest_call(
                "think_card",
                "Create an update card listing impacted decisions/hypotheses and the next decisive test.",
                "medium",
                json!({
                    "workspace": scope.workspace,
                    "branch": scope.branch,
                    "trace_doc": scope.trace_doc,
                    "graph_doc": scope.graph_doc,
                    "card": {
                        "type": "update",
                        "title": format!("Assumption changed: {}", issue.title),
                        "text": "List impacted decisions/hypotheses, then define ONE decisive experiment to restore confidence.",
                        "status": "open",
                        "tags": ["bm6", "assumption"]
                    },
                    "supports": [issue.id]
                }),
            )],
            issue.ts_ms,
        ));
    }

    // ===== BM9: Reasoning patterns (deterministic playbooks) =====
    // Detect classic A vs B framing and suggest criteria matrix as a low-priority backup.
    let mut tradeoff_candidate: Option<(&str, i64)> = None;
    for card in by_id.values() {
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(ty, "question" | "decision") {
            continue;
        }
        if card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            != "open"
        {
            continue;
        }

        let mut matched = false;
        if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
            matched |= looks_like_tradeoff_text(title);
        }
        if !matched && let Some(text) = card.get("text").and_then(|v| v.as_str()) {
            matched |= looks_like_tradeoff_text(text);
        }
        if !matched {
            continue;
        }

        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let replace = match tradeoff_candidate {
            None => true,
            Some((_prev_id, prev_ts)) => ts_ms > prev_ts || (ts_ms == prev_ts && id < _prev_id),
        };
        if replace {
            tradeoff_candidate = Some((id, ts_ms));
        }
    }

    if let Some((id, ts_ms)) = tradeoff_candidate {
        actions.push(action_at(
            "use_playbook",
            "low",
            "Load criteria matrix playbook (A vs B)".to_string(),
            Some("BM9: tradeoffs are cheaper with a criteria matrix.".to_string()),
            vec![ref_card(id)],
            vec![suggest_call(
                "think_playbook",
                "Get a deterministic criteria-matrix playbook skeleton.",
                "low",
                json!({ "workspace": scope.workspace, "name": "criteria_matrix" }),
            )],
            ts_ms,
        ));
    }

    // ===== BM5: Executable tests (next runnable test) =====
    // ===== BM8: Time-decay (stale evidence → recommend re-run) =====
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RunnableEvidenceState {
        Missing,
        Stale,
        Fresh,
    }

    #[derive(Clone, Debug)]
    struct RunnableTestCandidate {
        test_ts_ms: i64,
        test_id: String,
        cmd: String,
        state: RunnableEvidenceState,
        evidence_latest_ts_ms: Option<i64>,
    }

    let mut runnable_tests: Vec<RunnableTestCandidate> = Vec::new();
    for card in by_id.values() {
        if card.get("type").and_then(|v| v.as_str()) != Some("test") {
            continue;
        }
        if card
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("open")
            != "open"
        {
            continue;
        }
        let Some(test_id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(cmd) = extract_cmd_from_test_card(card) else {
            continue;
        };
        let test_ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);

        let mut evidence_latest_ts_ms: Option<i64> = None;
        if let Some(supporters) = incoming_supports.get(test_id) {
            for from_id in supporters {
                let Some(from_card) = by_id.get(from_id).copied() else {
                    continue;
                };
                if from_card.get("type").and_then(|v| v.as_str()) != Some("evidence") {
                    continue;
                }
                let ts = from_card
                    .get("last_ts_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                evidence_latest_ts_ms = Some(evidence_latest_ts_ms.unwrap_or(0).max(ts));
            }
        }

        let stale_after_ms = extract_stale_after_ms_from_test_card(card).unwrap_or_else(|| {
            let days = extract_stale_after_days_from_test_card(card).unwrap_or(30);
            let days = days.clamp(0, 3650);
            days.saturating_mul(ms_per_day())
        });

        let state = match evidence_latest_ts_ms {
            None => RunnableEvidenceState::Missing,
            Some(evidence_ts_ms) => {
                if reference_ts_ms > evidence_ts_ms.saturating_add(stale_after_ms) {
                    RunnableEvidenceState::Stale
                } else {
                    RunnableEvidenceState::Fresh
                }
            }
        };

        runnable_tests.push(RunnableTestCandidate {
            test_ts_ms,
            test_id: test_id.to_string(),
            cmd,
            state,
            evidence_latest_ts_ms,
        });
    }

    runnable_tests.sort_by(|a, b| {
        let state_rank = |s: RunnableEvidenceState| match s {
            RunnableEvidenceState::Missing => 0,
            RunnableEvidenceState::Stale => 1,
            RunnableEvidenceState::Fresh => 2,
        };
        state_rank(a.state)
            .cmp(&state_rank(b.state))
            .then_with(|| b.test_ts_ms.cmp(&a.test_ts_ms))
            .then_with(|| a.test_id.cmp(&b.test_id))
    });

    let runnable_total = runnable_tests.len();
    if let Some(best) = runnable_tests
        .iter()
        .find(|c| c.state != RunnableEvidenceState::Fresh)
    {
        let label = by_id
            .get(best.test_id.as_str())
            .map(|c| shorten(&card_label(c), 64))
            .unwrap_or_else(|| best.test_id.to_string());
        let cmd_short = shorten(best.cmd.as_str(), 96);

        if best.state == RunnableEvidenceState::Stale {
            let age_ms = best
                .evidence_latest_ts_ms
                .map(|ts| reference_ts_ms.saturating_sub(ts))
                .unwrap_or(0);
            let age_days = if ms_per_day() == 0 {
                0
            } else {
                age_ms / ms_per_day()
            };
            signals.push(signal_at(
                "BM8_EVIDENCE_STALE",
                "warning",
                format!("Evidence looks stale for runnable test: {label} (age≈{age_days}d)"),
                vec![ref_card(best.test_id.as_str())],
                best.test_ts_ms,
            ));
        }

        let (priority, why) = match best.state {
            RunnableEvidenceState::Missing => (
                "high",
                Some("BM5: runnable test has no linked evidence in current slice.".to_string()),
            ),
            RunnableEvidenceState::Stale => (
                "medium",
                Some(
                    "BM8: runnable test has evidence, but it looks stale in this slice."
                        .to_string(),
                ),
            ),
            RunnableEvidenceState::Fresh => ("low", None),
        };

        let calls = vec![suggest_call(
            "think_card",
            "After running the test, capture evidence and link it to the test card.",
            "high",
            json!({
                "workspace": scope.workspace,
                "branch": scope.branch,
                "trace_doc": scope.trace_doc,
                "graph_doc": scope.graph_doc,
                "card": {
                    "type": "evidence",
                    "title": format!("Evidence: {label}"),
                    "text": "Paste the command output and an artifact link; keep it factual.",
                    "status": "open",
                    "tags": ["bm5"],
                    "meta": { "run": { "cmd": best.cmd.as_str() } }
                },
                "supports": [best.test_id.as_str()]
            }),
        )];

        actions.push(action_at(
            "run_test",
            priority,
            format!("Run test: {label} ({cmd_short})"),
            why,
            vec![ref_card(best.test_id.as_str())],
            calls,
            best.test_ts_ms,
        ));
    } else if runnable_total > 0 {
        signals.push(signal_at(
            "BM5_RUNNABLE_TESTS_FRESH",
            "info",
            format!("{runnable_total} runnable tests detected; evidence appears fresh in slice."),
            Vec::new(),
            reference_ts_ms,
        ));
    }

    // ===== BM10: Meta-reasoning hooks (stuck + bias risk) =====
    let has_progress = trace_has_progress_signal(trace_entries);
    let mut recent_think_cards = 0usize;
    for entry in trace_entries.iter().rev().take(12) {
        if entry.get("kind").and_then(|v| v.as_str()) == Some("note")
            && entry.get("format").and_then(|v| v.as_str()) == Some("think_card")
        {
            recent_think_cards += 1;
        }
    }

    if !has_progress && recent_think_cards >= 6 {
        signals.push(signal_at(
            "BM10_STUCK_NO_EVIDENCE",
            "warning",
            "No recent evidence captured in trace slice; consider pivoting to the smallest runnable test.".to_string(),
            Vec::new(),
            reference_ts_ms,
        ));
        actions.push(action_at(
            "use_playbook",
            "medium",
            "Load debug playbook (reframe → test → evidence)".to_string(),
            Some(
                "BM10: trace suggests low progress; a structured reset is cheaper than spinning."
                    .to_string(),
            ),
            Vec::new(),
            vec![
                suggest_call(
                    "think_playbook",
                    "Get a deterministic debug playbook skeleton.",
                    "medium",
                    json!({ "workspace": scope.workspace, "name": "debug" }),
                ),
                suggest_call(
                    "think_playbook",
                    "If you're looping, load the breakthrough playbook (inversion → 10x lever → decisive test).",
                    "low",
                    json!({ "workspace": scope.workspace, "name": "breakthrough" }),
                ),
            ],
            reference_ts_ms,
        ));
    }

    // ===== BM7: Counter-argument generation (steelman) =====
    // Find a concrete target that has supports but no blocks edges in this slice.
    // This is both a bias alert (BM10) and a prompt to add a counter-position (BM7).
    #[derive(Clone, Debug)]
    struct CounterTarget<'a> {
        id: &'a str,
        card: &'a Value,
        ts_ms: i64,
        supports: usize,
    }

    let mut counter_targets = Vec::<CounterTarget<'_>>::new();
    for card in by_id.values() {
        let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(ty, "hypothesis" | "decision") {
            continue;
        }
        // Counter-hypotheses are themselves the "blocks" side of the dialectic. Requiring a
        // counter-position for a counter-position leads to infinite regress, so we treat cards
        // tagged as `counter` as exempt from BM10.
        if card_has_tag(card, "counter") {
            continue;
        }
        if !card_status_is_active_for_discipline(card) {
            continue;
        }
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let supports = incoming_supports.get(id).map(|v| v.len()).unwrap_or(0);
        let blocks = incoming_blocks.get(id).map(|v| v.len()).unwrap_or(0);
        if supports == 0 || blocks > 0 {
            continue;
        }
        let ts_ms = card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        counter_targets.push(CounterTarget {
            id,
            card,
            ts_ms,
            supports,
        });
    }
    counter_targets.sort_by(|a, b| {
        b.supports
            .cmp(&a.supports)
            .then_with(|| b.ts_ms.cmp(&a.ts_ms))
            .then_with(|| a.id.cmp(b.id))
    });

    if let Some(target) = counter_targets.into_iter().next() {
        let label = shorten(&card_label(target.card), 64);

        signals.push(signal_at(
            "BM10_NO_COUNTER_EDGES",
            "info",
            format!(
                "Card has supports but no blocks edges (in slice); add a counter-position: {label}"
            ),
            vec![ref_card(target.id)],
            target.ts_ms.max(reference_ts_ms),
        ));

        actions.push(action_at(
            "add_counter_hypothesis",
            "medium",
            format!("Steelman a counter-hypothesis for: {label}"),
            Some(
                "BM7: counter-arguments reduce confirmation bias and sharpen the next decisive test."
                    .to_string(),
            ),
            vec![ref_card(target.id)],
            {
                let mut tags = vec!["bm7".to_string(), "counter".to_string()];
                tags.extend(card_value_anchor_tags(target.card));
                tags = bm_core::graph::normalize_tags(&tags).unwrap_or(tags);

                vec![
                    suggest_call(
                        "think_playbook",
                        "Load a short skeptic loop (counter-hypothesis → falsifier → stop criteria).",
                        "low",
                        json!({ "workspace": scope.workspace, "name": "skeptic" }),
                    ),
                    suggest_call(
                        "think_card",
                        "Write the strongest opposite hypothesis + cheapest falsifier + stop criteria.",
                        "medium",
                        json!({
                            "workspace": scope.workspace,
                            "branch": scope.branch,
                            "trace_doc": scope.trace_doc,
                            "graph_doc": scope.graph_doc,
                            "card": {
                                "type": "hypothesis",
                                "title": format!("Counter-hypothesis: {label}"),
                                "text": "Steelman the opposite case.\n- Minimal falsifying test: (what would disprove this quickly?)\n- Stop criteria (time/budget/signal): (when do we stop debating?)",
                                "status": "open",
                                "tags": tags
                            },
                            "blocks": [target.id]
                        }),
                    ),
                ]
            },
            target.ts_ms.max(reference_ts_ms),
        ));
    }

    // Deterministic ordering + budgets.
    signals.sort_by(|a, b| {
        b.severity_rank
            .cmp(&a.severity_rank)
            .then_with(|| b.sort_ts_ms.cmp(&a.sort_ts_ms))
            .then_with(|| a.code.cmp(b.code))
            .then_with(|| a.message.cmp(&b.message))
    });
    actions.sort_by(|a, b| {
        b.priority_rank
            .cmp(&a.priority_rank)
            .then_with(|| b.sort_ts_ms.cmp(&a.sort_ts_ms))
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.title.cmp(&b.title))
    });

    let signals_total = signals.len();
    let actions_total = actions.len();
    let mut truncated = false;

    let signals_out = if limits.signals_limit == 0 {
        Vec::new()
    } else {
        let limit = limits.signals_limit.max(1);
        if signals.len() > limit {
            truncated = true;
        }
        signals
            .into_iter()
            .take(limit)
            .map(|s| {
                json!({
                    "code": s.code,
                    "severity": s.severity,
                    "message": s.message,
                    "refs": s.refs.into_iter().map(|r| json!({"kind": r.kind, "id": r.id})).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    };

    let actions_out = if limits.actions_limit == 0 {
        Vec::new()
    } else {
        let limit = limits.actions_limit.max(1);
        if actions.len() > limit {
            truncated = true;
        }
        actions
            .into_iter()
            .take(limit)
            .map(|a| {
                json!({
                    "kind": a.kind,
                    "priority": a.priority,
                    "title": a.title,
                    "why": a.why,
                    "refs": a.refs.into_iter().map(|r| json!({"kind": r.kind, "id": r.id})).collect::<Vec<_>>(),
                    "calls": a.calls
                })
            })
            .collect::<Vec<_>>()
    };

    if signals_out.is_empty() && actions_out.is_empty() {
        return None;
    }

    Some(json!({
        "version": REASONING_ENGINE_VERSION,
        "signals_total": signals_total,
        "actions_total": actions_total,
        "signals": signals_out,
        "actions": actions_out,
        "truncated": truncated
    }))
}
