#![forbid(unsafe_code)]

pub(in crate::store) fn merge_meta_json(
    existing_meta_json: Option<&str>,
    from_branch: &str,
    from_seq: i64,
    from_ts_ms: i64,
) -> String {
    let payload = format!(
        r#"{{"from":"{}","from_seq":{},"from_ts_ms":{}}}"#,
        json_escape(from_branch),
        from_seq,
        from_ts_ms
    );

    let Some(raw) = existing_meta_json else {
        return format!(r#"{{"_merge":{payload}}}"#);
    };

    let trimmed = raw.trim();
    if looks_like_json_object(trimmed) {
        if trimmed == "{}" {
            return format!(r#"{{"_merge":{payload}}}"#);
        }

        if trimmed.contains("\"_merge\"") {
            return format!(r#"{{"_merge":{payload},"_meta":{trimmed}}}"#);
        }

        let mut out = trimmed.to_string();
        out.pop(); // remove trailing '}'
        if !out.trim_end().ends_with('{') {
            out.push(',');
        }
        out.push_str(&format!(r#""_merge":{payload}}}"#));
        return out;
    }

    format!(
        r#"{{"_merge":{payload},"_meta_raw":"{}"}}"#,
        json_escape(trimmed)
    )
}

pub(in crate::store) fn looks_like_json_object(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}')
}

pub(in crate::store) fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
