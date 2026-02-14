#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Default)]
struct ExecutorPolicy {
    prefer: Vec<String>,
    forbid: HashSet<String>,
    min_profile: Option<String>,
}

fn parse_policy(value: &Value) -> ExecutorPolicy {
    let mut policy = ExecutorPolicy::default();
    let Some(obj) = value.as_object() else {
        return policy;
    };
    if let Some(prefer) = obj.get("prefer").and_then(|v| v.as_array()) {
        policy.prefer = prefer
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(forbid) = obj.get("forbid").and_then(|v| v.as_array()) {
        policy.forbid = forbid
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<HashSet<_>>();
    }
    if let Some(min_profile) = obj.get("min_profile").and_then(|v| v.as_str()) {
        policy.min_profile = Some(min_profile.to_string());
    }
    policy
}

fn profile_rank(profile: &str) -> u8 {
    match profile.to_ascii_lowercase().as_str() {
        "fast" => 0,
        "deep" => 1,
        "audit" => 2,
        "xhigh" => 3,
        _ => 0,
    }
}

pub(super) fn auto_route_executor(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    executor_profile: &str,
    expected_artifacts: &[String],
    policy_value: &Value,
) -> Option<Value> {
    let policy = parse_policy(policy_value);
    let now_ms = crate::support::now_ms_i64();
    let list = server
        .store
        .runner_leases_list_active(
            workspace,
            now_ms,
            bm_storage::RunnerLeasesListRequest {
                limit: 50,
                status: None,
            },
        )
        .ok()?;

    let mut candidates = Vec::<(u8, u8, String, String)>::new(); // (prefer_rank, availability_rank, runner_id, executor)
    for runner in list.runners {
        let meta = server
            .store
            .runner_lease_get(
                workspace,
                bm_storage::RunnerLeaseGetRequest {
                    runner_id: runner.runner_id.clone(),
                },
            )
            .ok()
            .flatten()
            .and_then(|row| row.meta_json)
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or(Value::Null);
        let executors = meta
            .get("executors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["codex".to_string()]);
        let profiles = meta
            .get("profiles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["fast".to_string(), "deep".to_string(), "audit".to_string()]);
        let supports_artifacts = meta
            .get("supports_artifacts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let availability_rank = if runner.status == "idle" { 0 } else { 1 };
        for executor in executors {
            if policy.forbid.contains(&executor) {
                continue;
            }
            if !profiles.iter().any(|p| p == executor_profile) {
                continue;
            }
            if let Some(min_profile) = &policy.min_profile
                && profile_rank(executor_profile) < profile_rank(min_profile)
            {
                continue;
            }
            if !expected_artifacts.is_empty() && !supports_artifacts.is_empty() {
                let missing = expected_artifacts
                    .iter()
                    .any(|item| !supports_artifacts.iter().any(|v| v == item));
                if missing {
                    continue;
                }
            }
            let prefer_rank = policy
                .prefer
                .iter()
                .position(|v| v == &executor)
                .unwrap_or(usize::MAX) as u8;
            candidates.push((
                prefer_rank,
                availability_rank,
                runner.runner_id.clone(),
                executor,
            ));
        }
    }

    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let (_, _, runner_id, executor) = candidates.first()?.clone();
    Some(json!({
        "selected_executor": executor,
        "selected_runner_id": runner_id,
        "policy": {
            "prefer": policy.prefer,
            "forbid": policy.forbid.iter().cloned().collect::<Vec<_>>(),
            "min_profile": policy.min_profile
        }
    }))
}
