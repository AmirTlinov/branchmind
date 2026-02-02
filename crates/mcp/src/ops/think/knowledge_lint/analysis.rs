#![forbid(unsafe_code)]

use super::model::{
    CrossDuplicateGroup, DuplicateGroup, Entry, OverloadedKeySummary, OverloadedOutliersGroup,
};
use serde_json::{Value, json};

pub(crate) struct OverloadedAnalysis {
    pub(crate) issues: Vec<Value>,
    pub(crate) outliers: Vec<OverloadedOutliersGroup>,
    pub(crate) overloaded_keys: Vec<OverloadedKeySummary>,
}

pub(crate) fn analyze_duplicate_content_same_anchor(
    entries: &[Entry],
) -> (Vec<Value>, Vec<DuplicateGroup>) {
    let mut issues = Vec::<Value>::new();
    let mut duplicate_groups = Vec::<DuplicateGroup>::new();

    // High-confidence duplicates: same normalized content, same anchor, different keys.
    let mut by_anchor_hash = std::collections::BTreeMap::<(String, u64), Vec<Entry>>::new();
    for entry in entries.iter().cloned() {
        by_anchor_hash
            .entry((entry.anchor_id.clone(), entry.content_hash))
            .or_default()
            .push(entry);
    }
    for ((anchor_id, content_hash), mut group) in by_anchor_hash {
        group.sort_by(|a, b| {
            a.created_at_ms
                .cmp(&b.created_at_ms)
                .then_with(|| a.key.cmp(&b.key))
                .then_with(|| a.card_id.cmp(&b.card_id))
        });
        let mut keys = group
            .iter()
            .map(|e| e.key.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if keys.len() < 2 {
            continue;
        }
        keys.sort();
        let mut card_ids = group
            .iter()
            .map(|e| e.card_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        card_ids.sort();

        let recommended = group.first().expect("non-empty group");
        let recommended_key = recommended.key.clone();
        let recommended_card_id = recommended.card_id.clone();

        issues.push(json!({
            "severity": "warning",
            "code": "KNOWLEDGE_DUPLICATE_CONTENT_SAME_ANCHOR",
            "message": format!(
                "Duplicate knowledge content under one anchor: {} has multiple keys with identical content.",
                anchor_id
            ),
            "evidence": {
                "anchor_id": anchor_id,
                "keys": keys,
                "card_ids": card_ids,
                "content_hash": format!("{content_hash:016x}"),
                "recommended_key": recommended_key,
                "recommended_card_id": recommended_card_id
            }
        }));

        duplicate_groups.push(DuplicateGroup {
            anchor_id,
            content_hash,
            keys,
            card_ids,
            recommended_key,
        });
    }

    (issues, duplicate_groups)
}

pub(crate) fn analyze_duplicate_content_same_key_across_anchors(
    entries: &[Entry],
) -> (Vec<Value>, Vec<CrossDuplicateGroup>) {
    let mut issues = Vec::<Value>::new();
    let mut groups = Vec::<CrossDuplicateGroup>::new();

    // Duplicate content for the same key across anchors (often “shared knowledge”).
    //
    // IMPORTANT: We only report this when a content-hash is represented under a *single key*
    // across anchors. If the same hash appears under multiple keys, we treat that as key drift
    // (handled by `analyze_duplicate_content_across_anchors_multiple_keys`) and avoid doubling
    // the surface area (noise).
    let mut keys_by_hash =
        std::collections::BTreeMap::<u64, std::collections::BTreeSet<String>>::new();
    for entry in entries.iter() {
        keys_by_hash
            .entry(entry.content_hash)
            .or_default()
            .insert(entry.key.clone());
    }

    let mut by_key_hash = std::collections::BTreeMap::<(String, u64), Vec<Entry>>::new();
    for entry in entries.iter().cloned() {
        by_key_hash
            .entry((entry.key.clone(), entry.content_hash))
            .or_default()
            .push(entry);
    }
    for ((key, content_hash), mut group) in by_key_hash {
        let keys_for_hash = keys_by_hash
            .get(&content_hash)
            .map(|s| s.len())
            .unwrap_or(0);
        if keys_for_hash > 1 {
            continue;
        }

        group.sort_by(|a, b| {
            a.created_at_ms
                .cmp(&b.created_at_ms)
                .then_with(|| a.anchor_id.cmp(&b.anchor_id))
                .then_with(|| a.card_id.cmp(&b.card_id))
        });
        let anchors = group
            .iter()
            .map(|e| e.anchor_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let anchor_count = anchors.len();
        if anchor_count < 2 {
            continue;
        }
        let card_ids = group
            .iter()
            .map(|e| e.card_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let anchors_sample = anchors.iter().take(12).cloned().collect::<Vec<_>>();
        let card_ids_sample = card_ids.iter().take(12).cloned().collect::<Vec<_>>();
        let recommended_anchor_id = group
            .first()
            .map(|e| e.anchor_id.clone())
            .unwrap_or_else(|| anchors[0].clone());
        let recommended_key = key.clone();
        let key_for_issue = key.clone();
        let recommended_anchor_id_for_issue = recommended_anchor_id.clone();

        issues.push(json!({
            "severity": "info",
            "code": "KNOWLEDGE_DUPLICATE_CONTENT_SAME_KEY_ACROSS_ANCHORS",
            "message": format!(
                "Key is reused across anchors with identical content: k:{} appears in {} anchors.",
                key_for_issue,
                anchor_count
            ),
            "evidence": {
                "key": key_for_issue,
                "anchor_count": anchor_count,
                "anchors_sample": anchors_sample,
                "card_ids_sample": card_ids_sample,
                "recommended_anchor_id": recommended_anchor_id_for_issue,
                "content_hash": format!("{content_hash:016x}")
            }
        }));

        groups.push(CrossDuplicateGroup {
            content_hash,
            anchors,
            keys: vec![key],
            card_ids,
            recommended_anchor_id,
            recommended_key,
        });
    }

    (issues, groups)
}

pub(crate) fn analyze_duplicate_content_across_anchors_multiple_keys(
    entries: &[Entry],
) -> (Vec<Value>, Vec<CrossDuplicateGroup>) {
    let mut issues = Vec::<Value>::new();
    let mut groups = Vec::<CrossDuplicateGroup>::new();

    // Duplicate content across anchors under multiple keys (likely key drift / duplicate knowledge).
    //
    // We intentionally skip cases where only a single key is used across anchors: those are already
    // reported as `KNOWLEDGE_DUPLICATE_CONTENT_SAME_KEY_ACROSS_ANCHORS`.
    let mut by_hash_any = std::collections::BTreeMap::<u64, Vec<Entry>>::new();
    for entry in entries.iter().cloned() {
        by_hash_any
            .entry(entry.content_hash)
            .or_default()
            .push(entry);
    }
    for (content_hash, mut group) in by_hash_any {
        group.sort_by(|a, b| {
            a.created_at_ms
                .cmp(&b.created_at_ms)
                .then_with(|| a.anchor_id.cmp(&b.anchor_id))
                .then_with(|| a.key.cmp(&b.key))
                .then_with(|| a.card_id.cmp(&b.card_id))
        });
        if group.len() < 2 {
            continue;
        }

        let anchors = group
            .iter()
            .map(|e| e.anchor_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if anchors.len() < 2 {
            continue;
        }

        let keys_for_content = group
            .iter()
            .map(|e| e.key.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if keys_for_content.len() < 2 {
            continue;
        }

        let card_ids = group
            .iter()
            .map(|e| e.card_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let recommended = group.first().expect("non-empty group");
        let recommended_anchor_id = recommended.anchor_id.clone();
        let recommended_key = recommended.key.clone();
        let recommended_card_id = recommended.card_id.clone();

        let anchors_sample = anchors.iter().take(12).cloned().collect::<Vec<_>>();
        let keys_sample = keys_for_content
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        let card_ids_sample = card_ids.iter().take(12).cloned().collect::<Vec<_>>();

        issues.push(json!({
            "severity": "info",
            "code": "KNOWLEDGE_DUPLICATE_CONTENT_ACROSS_ANCHORS_MULTIPLE_KEYS",
            "message": format!(
                "Duplicate knowledge content across anchors under multiple keys: {} anchors, {} keys share identical content.",
                anchors.len(),
                keys_for_content.len()
            ),
            "evidence": {
                "anchor_count": anchors.len(),
                "key_count": keys_for_content.len(),
                "anchors_sample": anchors_sample,
                "keys_sample": keys_sample,
                "card_ids_sample": card_ids_sample,
                "content_hash": format!("{content_hash:016x}"),
                "recommended": {
                    "anchor_id": recommended_anchor_id,
                    "key": recommended_key,
                    "card_id": recommended_card_id
                }
            }
        }));

        groups.push(CrossDuplicateGroup {
            content_hash,
            anchors,
            keys: keys_for_content,
            card_ids,
            recommended_anchor_id,
            recommended_key,
        });
    }

    (issues, groups)
}

pub(crate) fn analyze_overloaded_keys(entries: &[Entry]) -> OverloadedAnalysis {
    // Precision-first: we only propose “outliers” when one variant clearly dominates. Otherwise we
    // keep this as an info-only “overloaded” signal (no strong consolidation claim).
    #[derive(Clone, Debug)]
    struct KeyAcc {
        anchors: std::collections::BTreeSet<String>,
        variants: std::collections::BTreeMap<u64, Vec<Entry>>,
    }

    let mut key_acc = std::collections::BTreeMap::<String, KeyAcc>::new();
    for entry in entries.iter().cloned() {
        let slot = key_acc.entry(entry.key.clone()).or_insert_with(|| KeyAcc {
            anchors: std::collections::BTreeSet::new(),
            variants: std::collections::BTreeMap::new(),
        });
        slot.anchors.insert(entry.anchor_id.clone());
        slot.variants
            .entry(entry.content_hash)
            .or_default()
            .push(entry);
    }

    let mut issues = Vec::<Value>::new();
    let mut outliers = Vec::<OverloadedOutliersGroup>::new();
    let mut overloaded_keys = Vec::<OverloadedKeySummary>::new();

    for (key, acc) in key_acc.iter() {
        let anchor_count = acc.anchors.len();
        let variant_count = acc.variants.len();
        if anchor_count < 2 || variant_count < 2 {
            continue;
        }

        let total_count = acc.variants.values().map(|v| v.len()).sum::<usize>();
        let anchors_sample = acc.anchors.iter().take(12).cloned().collect::<Vec<_>>();

        let mut variant_counts = acc
            .variants
            .iter()
            .map(|(hash, entries)| (entries.len(), *hash))
            .collect::<Vec<_>>();
        variant_counts.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let (dominant_count, dominant_hash) =
            variant_counts.first().cloned().unwrap_or((0usize, 0u64));

        // Dominance rule (integer math, deterministic):
        // - at least 3 total observations
        // - dominant appears at least twice
        // - dominant share >= 60%
        let has_dominant = total_count >= 3
            && dominant_count >= 2
            && dominant_count.saturating_mul(10) >= total_count.saturating_mul(6);

        if has_dominant {
            let dominant_entries = acc
                .variants
                .get(&dominant_hash)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let dominant_anchors = dominant_entries
                .iter()
                .map(|e| e.anchor_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .take(12)
                .collect::<Vec<_>>();
            let dominant_card_ids = dominant_entries
                .iter()
                .map(|e| e.card_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .take(12)
                .collect::<Vec<_>>();

            let mut outlier_variants = Vec::<Value>::new();
            let mut outlier_card_ids = std::collections::BTreeSet::<String>::new();
            for (_count, hash) in variant_counts.iter().cloned().skip(1).take(4) {
                let Some(entries) = acc.variants.get(&hash) else {
                    continue;
                };
                let anchors_sample = entries
                    .iter()
                    .map(|e| e.anchor_id.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .take(12)
                    .collect::<Vec<_>>();
                let card_ids_sample = entries
                    .iter()
                    .map(|e| e.card_id.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .take(12)
                    .collect::<Vec<_>>();
                for id in entries.iter().map(|e| e.card_id.clone()) {
                    outlier_card_ids.insert(id);
                }
                outlier_variants.push(json!({
                    "content_hash": format!("{hash:016x}"),
                    "count": entries.len(),
                    "anchors_sample": anchors_sample,
                    "card_ids_sample": card_ids_sample
                }));
            }

            issues.push(json!({
                "severity": "info",
                "code": "KNOWLEDGE_KEY_OVERLOADED_OUTLIERS",
                "message": format!(
                    "Key looks overloaded with outliers: k:{} has {} anchors, {} variants; one variant dominates ({} of {}).",
                    key,
                    anchor_count,
                    variant_count,
                    dominant_count,
                    total_count
                ),
                "evidence": {
                    "key": key,
                    "anchor_count": anchor_count,
                    "variant_count": variant_count,
                    "total_count": total_count,
                    "anchors_sample": anchors_sample,
                    "dominant": {
                        "content_hash": format!("{dominant_hash:016x}"),
                        "count": dominant_count,
                        "anchors_sample": dominant_anchors,
                        "card_ids_sample": dominant_card_ids
                    },
                    "outliers_sample": outlier_variants
                }
            }));

            outliers.push(OverloadedOutliersGroup {
                key: key.clone(),
                dominant_hash,
                dominant_count,
                total_count,
                outlier_card_ids: outlier_card_ids.into_iter().take(50).collect::<Vec<_>>(),
            });
        } else {
            let variants_sample = variant_counts
                .iter()
                .take(4)
                .map(|(count, hash)| {
                    json!({
                        "content_hash": format!("{hash:016x}"),
                        "count": count
                    })
                })
                .collect::<Vec<_>>();
            issues.push(json!({
                "severity": "info",
                "code": "KNOWLEDGE_KEY_OVERLOADED_ACROSS_ANCHORS",
                "message": format!(
                    "Key may be overloaded (reused with different content): k:{} has {} anchors and {} variants.",
                    key,
                    anchor_count,
                    variant_count
                ),
                "evidence": {
                    "key": key,
                    "anchor_count": anchor_count,
                    "variant_count": variant_count,
                    "total_count": total_count,
                    "anchors_sample": anchors_sample,
                    "variants_sample": variants_sample
                }
            }));
        }

        overloaded_keys.push(OverloadedKeySummary {
            key: key.clone(),
            anchor_count,
            variant_count,
        });
    }

    OverloadedAnalysis {
        issues,
        outliers,
        overloaded_keys,
    }
}
