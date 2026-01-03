#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"
        PRAGMA journal_mode=WAL;
        PRAGMA synchronous=NORMAL;
"#;
