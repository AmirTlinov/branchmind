#![forbid(unsafe_code)]

pub(crate) fn normalize_cmd(raw: &str) -> Result<String, String> {
    normalize_token(raw, true)
}

#[cfg(test)]
pub(crate) fn normalize_op(raw: &str) -> Result<String, String> {
    if raw.trim() == "call" {
        return Ok("call".to_string());
    }
    normalize_token(raw, false)
}

pub(crate) fn name_to_cmd_segments(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', ".")
}

fn normalize_token(raw: &str, require_dot: bool) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("must not be empty".to_string());
    }
    if trimmed.contains('-') {
        return Err("must not contain '-' (use underscores)".to_string());
    }
    let lowered = trimmed.to_ascii_lowercase();
    let parts: Vec<&str> = lowered.split('.').collect();
    if require_dot && parts.len() < 2 {
        return Err("must contain at least one '.'".to_string());
    }
    for part in parts.iter() {
        if part.is_empty() {
            return Err("must not contain empty segments".to_string());
        }
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            return Err("must not be empty".to_string());
        };
        if !first.is_ascii_lowercase() {
            return Err("segments must start with a-z".to_string());
        }
        for ch in chars {
            if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
                return Err("segments must be [a-z0-9_]".to_string());
            }
        }
    }
    Ok(lowered)
}
