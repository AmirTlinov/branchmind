#![forbid(unsafe_code)]

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) fn now_rfc3339() -> Value {
    Value::String(
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
    )
}

pub(crate) fn now_ms_i64() -> i64 {
    let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos();
    let ms = nanos / 1_000_000i128;
    if ms <= 0 {
        0
    } else if ms >= i64::MAX as i128 {
        i64::MAX
    } else {
        ms as i64
    }
}

pub(crate) fn ts_ms_to_rfc3339(ts_ms: i64) -> String {
    let nanos = (ts_ms as i128) * 1_000_000i128;
    let dt = OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    dt.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
