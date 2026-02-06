#![forbid(unsafe_code)]

pub(super) fn strip_markdown_list_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return rest.trim_start();
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return rest.trim_start();
    }
    let bytes = trimmed.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && idx + 1 < bytes.len() && bytes[idx] == b'.' && bytes[idx + 1] == b' ' {
        return trimmed[idx + 2..].trim_start();
    }
    trimmed
}

pub(super) fn looks_like_placeholder(value: &str) -> bool {
    let v = value.trim();
    v.is_empty() || v.contains("<fill")
}

pub(super) fn strip_markdownish_prefixes(line: &str) -> &str {
    let mut s = line.trim_start();

    if let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }

    for prefix in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest.trim_start();
        }
    }

    let bytes = s.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0
        && idx + 1 < bytes.len()
        && (bytes[idx] == b'.' || bytes[idx] == b')')
        && bytes[idx + 1] == b' '
    {
        return s[(idx + 2)..].trim_start();
    }

    s
}

pub(super) fn looks_like_tradeoff_text(value: &str) -> bool {
    let s = value.trim().to_ascii_lowercase();
    if s.is_empty() {
        return false;
    }
    s.contains(" vs ")
        || s.contains(" versus ")
        || s.contains("tradeoff")
        || s.contains("trade-off")
        || s.contains("a/b")
}
