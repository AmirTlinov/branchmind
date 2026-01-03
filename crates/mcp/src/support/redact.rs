#![forbid(unsafe_code)]

use serde_json::Value;

const SENSITIVE_KEYWORDS: [&str; 8] = [
    "token",
    "apikey",
    "api_key",
    "secret",
    "password",
    "private_key",
    "authorization",
    "bearer",
];

pub(crate) fn redact_value(value: &mut Value, depth: usize) {
    if depth == 0 {
        return;
    }
    match value {
        Value::String(s) => {
            let redacted = redact_text(s);
            if redacted != *s {
                *s = redacted;
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item, depth - 1);
            }
        }
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if is_sensitive_key(&key) {
                    map.insert(key, Value::String("<redacted>".to_string()));
                } else if let Some(value) = map.get_mut(&key) {
                    redact_value(value, depth - 1);
                }
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYWORDS.iter().any(|token| lower.contains(token))
}

pub(crate) fn redact_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut out = text.to_string();
    out = redact_token_prefix(&out, "ghp_", 20);
    out = redact_token_prefix(&out, "github_pat_", 20);
    out = redact_token_prefix(&out, "sk-", 20);
    for key in ["token", "apikey", "api_key", "secret", "password"] {
        out = redact_query_param(&out, key);
    }
    out = redact_bearer_token(&out);
    out = redact_private_key_block(&out);
    out
}

fn redact_token_prefix(input: &str, prefix: &str, min_tail: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    while i < bytes.len() {
        if bytes[i..].starts_with(prefix_bytes) {
            let start = i;
            let mut j = i + prefix_bytes.len();
            while j < bytes.len() && is_token_char(bytes[j]) {
                j += 1;
            }
            if j - start >= prefix_bytes.len() + min_tail {
                out.push_str("<redacted>");
            } else {
                out.push_str(&input[start..j]);
            }
            i = j;
        } else {
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn redact_query_param(input: &str, key: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let pattern = format!("{key}=");
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(pos) = lower[i..].find(&pattern) {
        let start = i + pos;
        let value_start = start + pattern.len();
        out.push_str(&input[i..value_start]);
        let mut j = value_start;
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_whitespace() || b == b'&' || b == b';' {
                break;
            }
            j += 1;
        }
        out.push_str("<redacted>");
        i = j;
    }
    out.push_str(&input[i..]);
    out
}

fn redact_bearer_token(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let needle = "bearer ";
    while let Some(pos) = lower[i..].find(needle) {
        let start = i + pos;
        let token_start = start + needle.len();
        out.push_str(&input[i..token_start]);
        let mut j = token_start;
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_whitespace() {
                break;
            }
            j += 1;
        }
        out.push_str("<redacted>");
        i = j;
    }
    out.push_str(&input[i..]);
    out
}

pub(crate) fn redact_private_key_block(input: &str) -> String {
    if !input.contains("PRIVATE KEY") {
        return input.to_string();
    }
    let begin = "-----BEGIN ";
    let end = "-----END ";
    let Some(start) = input.find(begin) else {
        return "<redacted>".to_string();
    };
    let Some(end_pos) = input[start..].find(end) else {
        return "<redacted>".to_string();
    };
    let end_abs = start + end_pos;
    let end_line = input[end_abs..]
        .find("-----")
        .map(|p| end_abs + p + 5)
        .unwrap_or(input.len());
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..start]);
    out.push_str("<redacted>");
    out.push_str(&input[end_line..]);
    out
}
