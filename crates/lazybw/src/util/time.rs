//! Human date and time formatting.
//!
//! Dates arrive from `bw` as ISO 8601 strings and are kept verbatim in the
//! models. These helpers only read them.

use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone, Utc};

/// Parses an ISO 8601 timestamp as sent by `bw` (with or without fractional
/// seconds and with `Z` or an offset).
#[must_use]
pub fn parse_iso(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(|n| Utc.from_utc_datetime(&n));
    }
    None
}

/// Relative time, e.g. `just now`, `2 min ago`, `3 hours ago`, `yesterday`,
/// `3 days ago`, `4 months ago`, `2 years ago`.
#[must_use]
pub fn relative(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds();
    if seconds < 0 {
        return "just now".to_owned();
    }
    match seconds {
        0..=44 => "just now".to_owned(),
        45..=89 => "1 min ago".to_owned(),
        90..=2699 => format!("{} min ago", seconds / 60),
        2700..=5399 => "1 hour ago".to_owned(),
        5400..=79_199 => format!("{} hours ago", seconds / 3600),
        79_200..=129_599 => "yesterday".to_owned(),
        129_600..=2_591_999 => format!("{} days ago", seconds / 86_400),
        2_592_000..=5_183_999 => "1 month ago".to_owned(),
        5_184_000..=31_535_999 => format!("{} months ago", seconds / 2_592_000),
        31_536_000..=63_071_999 => "1 year ago".to_owned(),
        _ => format!("{} years ago", seconds / 31_536_000),
    }
}

/// Formats a timestamp for the list subtitle.
#[must_use]
pub fn relative_from_str(now: DateTime<Utc>, value: &str) -> Option<String> {
    parse_iso(value).map(|then| relative(now, then))
}

/// Metadata date: `12 Mar 2023` by default, `2023-03-12` for the ISO style.
#[must_use]
pub fn date(_now: DateTime<Utc>, value: &str, iso: bool) -> Option<String> {
    let local = parse_iso(value)?.with_timezone(&Local);
    Some(if iso {
        format!("{:04}-{:02}-{:02}", local.year(), local.month(), local.day())
    } else {
        format!("{} {} {}", local.day(), month_name(local.month()), local.year())
    })
}

/// 24 hour clock time, e.g. `14:32`.
#[must_use]
pub fn clock(now: DateTime<Utc>) -> String {
    now.with_timezone(&Local).format("%H:%M").to_string()
}

/// Thousands separators: `3812` → `3,812`.
#[must_use]
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A stable relative description used in `:messages`.
#[must_use]
pub fn duration_words(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

fn month_name(month: u32) -> &'static str {
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    MONTHS.get(month.saturating_sub(1) as usize).copied().unwrap_or("Jan")
}

/// The fixed clock used by tests: 2026-10-30T14:00:00Z.
#[must_use]
pub fn test_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 30, 14, 0, 0).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days_ago: i64) -> DateTime<Utc> {
        test_now() - chrono::Duration::days(days_ago)
    }

    #[test]
    fn relative_buckets() {
        assert_eq!(relative(test_now(), test_now()), "just now");
        assert_eq!(relative(test_now(), test_now() - chrono::Duration::seconds(30)), "just now");
        assert_eq!(relative(test_now(), test_now() - chrono::Duration::minutes(2)), "2 min ago");
        assert_eq!(relative(test_now(), test_now() - chrono::Duration::hours(3)), "3 hours ago");
        assert_eq!(relative(test_now(), at(1)), "yesterday");
        assert_eq!(relative(test_now(), at(3)), "3 days ago");
        assert_eq!(relative(test_now(), at(120)), "4 months ago");
        assert_eq!(relative(test_now(), at(800)), "2 years ago");
    }

    #[test]
    fn parses_bw_dates() {
        assert!(parse_iso("2026-03-01T10:00:00.000Z").is_some());
        assert!(parse_iso("2026-03-01T10:00:00Z").is_some());
        assert!(parse_iso("2026-03-01T10:00:00").is_some());
        assert!(parse_iso("2026-03-01").is_some());
        assert!(parse_iso("not a date").is_none());
    }

    #[test]
    fn formats_dates_and_clock() {
        assert_eq!(date(test_now(), "2023-03-12T09:30:00Z", false), Some("12 Mar 2023".into()));
        assert_eq!(date(test_now(), "2023-03-12T09:30:00Z", true), Some("2023-03-12".into()));
        assert_eq!(clock(test_now()).len(), 5);
    }

    #[test]
    fn thousands_separator() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(3812), "3,812");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
