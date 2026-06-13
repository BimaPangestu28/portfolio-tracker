//! Time helpers: the assistant speaks WIB (UTC+7), storage is UTC.

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

/// WIB (Asia/Jakarta) is UTC+7 year-round — no DST, so a fixed offset is safe.
pub fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).expect("+07:00 is a valid offset")
}

/// Format a UTC instant the way the assistant tables store timestamps:
/// second precision, trailing Z. One format everywhere keeps lexicographic
/// order equal to chronological order in SQL comparisons.
pub fn to_db_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parse a datetime from a tool argument: RFC3339 (any offset) or a naive
/// "YYYY-MM-DDTHH:MM[:SS]" assumed WIB. Returns UTC; None when unparseable.
pub fn parse_tool_datetime(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M"))
        .ok()?;
    wib().from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Epoch-ms of 23:59:59 WIB on the WIB-local date of `now`. Used to bound a
/// "due today" window for tasks whose due dates are epoch ms.
pub fn end_of_today_wib_ms(now: DateTime<Utc>) -> i64 {
    let today = now.with_timezone(&wib()).date_naive();
    let end = today.and_hms_opt(23, 59, 59).expect("23:59:59 is valid");
    wib().from_local_datetime(&end).single().expect("WIB has no DST gaps").timestamp_millis()
}

/// Render a stored UTC timestamp as WIB for user-facing text. Unparseable
/// input is returned as-is (display helper — never fails).
pub fn to_wib_display(raw: &str) -> String {
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => dt.with_timezone(&wib()).format("%Y-%m-%d %H:%M WIB").to_string(),
        Err(_) => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_db_utc_uses_second_precision_z_format() {
        let dt = Utc.with_ymd_and_hms(2026, 6, 12, 2, 0, 0).unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
    }

    #[test]
    fn parses_rfc3339_with_offset_to_utc() {
        // 09:00 WIB == 02:00 UTC
        let dt = parse_tool_datetime("2026-06-12T09:00:00+07:00").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
    }

    #[test]
    fn parses_naive_datetime_as_wib() {
        let dt = parse_tool_datetime("2026-06-12T09:00").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
        let dt = parse_tool_datetime("2026-06-12T09:00:30").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:30Z");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_tool_datetime("besok jam 9").is_none());
        assert!(parse_tool_datetime("2026-06-12").is_none());
    }

    #[test]
    fn renders_stored_utc_as_wib() {
        assert_eq!(to_wib_display("2026-06-12T02:00:00Z"), "2026-06-12 09:00 WIB");
        // Unparseable values pass through untouched rather than panicking.
        assert_eq!(to_wib_display("oops"), "oops");
    }

    #[test]
    fn end_of_today_wib_is_2359_local() {
        let now = Utc.with_ymd_and_hms(2026, 6, 12, 20, 0, 0).unwrap();
        let end_ms = end_of_today_wib_ms(now);
        let expected = Utc.with_ymd_and_hms(2026, 6, 13, 16, 59, 59).unwrap().timestamp_millis();
        assert_eq!(end_ms, expected);
    }
}

/// Indonesian weekday name (used by proactive plan/briefing/review).
pub fn weekday_id(day: chrono::Weekday) -> &'static str {
    match day {
        chrono::Weekday::Mon => "Senin",
        chrono::Weekday::Tue => "Selasa",
        chrono::Weekday::Wed => "Rabu",
        chrono::Weekday::Thu => "Kamis",
        chrono::Weekday::Fri => "Jumat",
        chrono::Weekday::Sat => "Sabtu",
        chrono::Weekday::Sun => "Minggu",
    }
}
