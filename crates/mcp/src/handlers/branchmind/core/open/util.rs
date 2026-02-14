#![forbid(unsafe_code)]

use crate::*;

pub(super) fn summary_one_line(text: Option<&str>, title: Option<&str>, max_len: usize) -> String {
    let title = title.unwrap_or("").trim();
    if !title.is_empty() {
        return truncate_string(&redact_text(title), max_len);
    }
    let text = text.unwrap_or("").trim();
    if text.is_empty() {
        return String::new();
    }
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or(text);
    truncate_string(&redact_text(first.trim()), max_len)
}
