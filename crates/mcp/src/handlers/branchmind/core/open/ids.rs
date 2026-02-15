#![forbid(unsafe_code)]

pub(super) fn is_anchor_id(raw: &str) -> bool {
    raw.trim().to_ascii_lowercase().starts_with("a:")
}

pub(super) fn looks_like_repo_path_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.is_empty() {
        return false;
    }
    if raw.contains('@') {
        // Avoid conflicting with <doc>@<seq> refs.
        return false;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return true;
    }
    if raw == "." || raw == ".." || raw.starts_with("./") || raw.starts_with("../") {
        return true;
    }
    if raw == "~" || raw.starts_with("~/") {
        return true;
    }
    if raw.contains('/') || raw.contains('\\') {
        return true;
    }
    // Windows drive path: "C:\..." / "C:/..."
    if raw.len() >= 2 && raw.as_bytes().get(1) == Some(&b':') {
        return true;
    }
    false
}

pub(super) fn repo_rel_prefixes(repo_rel: &str) -> Vec<String> {
    let repo_rel = repo_rel.trim();
    if repo_rel == "." {
        return vec![".".to_string()];
    }
    let parts = repo_rel
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return vec![".".to_string()];
    }
    let mut out = Vec::<String>::new();
    for i in (1..=parts.len()).rev() {
        out.push(parts[0..i].join("/"));
    }
    out.push(".".to_string());
    out
}

pub(super) fn is_anchor_tag_any(tag: &str, anchor_ids: &[String]) -> bool {
    let tag = tag.trim();
    if tag.is_empty() {
        return false;
    }
    anchor_ids
        .iter()
        .any(|id| tag.eq_ignore_ascii_case(id.as_str()))
}

pub(super) fn anchor_title_from_id(anchor_id: &str) -> String {
    let raw = anchor_id.trim();
    let Some(slug) = raw.strip_prefix("a:").or_else(|| raw.strip_prefix("A:")) else {
        return "Anchor".to_string();
    };
    let words = slug
        .split('-')
        .filter(|w| !w.trim().is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        "Anchor".to_string()
    } else {
        words.join(" ")
    }
}

pub(super) fn parse_doc_entry_ref(raw: &str) -> Option<(String, i64)> {
    let raw = raw.trim();
    let (doc, seq_str) = raw.rsplit_once('@')?;
    let doc = doc.trim();
    let seq_str = seq_str.trim();
    if doc.is_empty() || seq_str.is_empty() {
        return None;
    }
    let seq = seq_str.parse::<i64>().ok()?;
    if seq < 0 {
        return None;
    }
    Some((doc.to_string(), seq))
}

pub(super) fn parse_job_event_ref(raw: &str) -> Option<(String, i64)> {
    let raw = raw.trim();
    let (job_id, seq_str) = raw.rsplit_once('@')?;
    let job_id = job_id.trim();
    let seq_str = seq_str.trim();
    if job_id.is_empty() || seq_str.is_empty() {
        return None;
    }
    if !job_id.starts_with("JOB-") {
        return None;
    }
    if !job_id
        .trim_start_matches("JOB-")
        .chars()
        .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let seq = seq_str.parse::<i64>().ok()?;
    if seq <= 0 {
        return None;
    }
    Some((job_id.to_string(), seq))
}

pub(super) fn parse_runner_ref(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let prefix = "runner:";
    if raw.len() <= prefix.len() {
        return None;
    }
    if !raw[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let runner_id = raw[prefix.len()..].trim();
    if runner_id.is_empty() {
        return None;
    }
    Some(runner_id.to_string())
}

pub(super) fn is_task_or_plan_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.contains('@') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("TASK-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    if let Some(rest) = raw.strip_prefix("PLAN-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

pub(super) fn is_task_id(raw: &str) -> bool {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("TASK-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

pub(super) fn is_slice_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.contains('@') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("SLC-") {
        // Slice IDs are generated as a fixed-width uppercase hex counter (e.g. SLC-0000000A),
        // so they may include A-F. Accept ASCII hex digits to keep `open SLC-*` stable.
        return rest.len() >= 8 && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    false
}

pub(super) fn is_step_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.contains('@') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("STEP-") {
        // Step IDs are generated as a fixed-width uppercase hex counter (e.g. STEP-0000000A),
        // so they may include A-F. Accept ASCII hex digits to keep `open STEP-*` stable.
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    false
}
