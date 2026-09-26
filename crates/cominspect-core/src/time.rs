//! Time helpers. All persistent timestamps are Unix time in milliseconds.

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Formats Unix milliseconds as RFC 3339 (UTC).
pub fn to_rfc3339(ms: i64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_default()
}

/// Parses an RFC 3339 timestamp into Unix milliseconds.
pub fn parse_rfc3339(value: &str) -> Option<i64> {
    OffsetDateTime::parse(value.trim(), &Rfc3339)
        .ok()
        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
}

/// Converts a Windows FILETIME (100 ns intervals since 1601-01-01) to Unix
/// milliseconds. Returns `None` for zero/invalid values.
pub fn filetime_to_unix_ms(filetime: u64) -> Option<i64> {
    const EPOCH_DIFFERENCE_100NS: u64 = 116_444_736_000_000_000;
    if filetime <= EPOCH_DIFFERENCE_100NS {
        return None;
    }
    Some(((filetime - EPOCH_DIFFERENCE_100NS) / 10_000) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_round_trip() {
        let ms = 1_790_000_000_123;
        let text = to_rfc3339(ms);
        assert!(text.starts_with("2026-"));
        assert_eq!(parse_rfc3339(&text), Some(ms));
        assert_eq!(parse_rfc3339("garbage"), None);
    }

    #[test]
    fn filetime_conversion() {
        // 2020-01-01T00:00:00Z
        assert_eq!(
            filetime_to_unix_ms(132_223_104_000_000_000),
            Some(1_577_836_800_000)
        );
        assert_eq!(filetime_to_unix_ms(0), None);
    }
}
