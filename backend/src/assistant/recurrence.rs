//! Advance recurring reminders to their next occurrence.

use chrono::{DateTime, Duration, Months, Utc};

/// The next occurrence after `current`, or None for one-shot/unknown patterns.
///
/// Monthly advancement clamps to the last day of shorter months and does NOT
/// restore the original day afterwards (Jan 31 -> Feb 28 -> Mar 28). Deliberate
/// trade-off: anchor-preserving recurrence isn't worth the extra state here.
pub fn next_occurrence(current: DateTime<Utc>, recurrence: &str) -> Option<DateTime<Utc>> {
    match recurrence {
        "daily" => Some(current + Duration::days(1)),
        "weekly" => Some(current + Duration::days(7)),
        "monthly" => current.checked_add_months(Months::new(1)),
        _ => None,
    }
}

/// Next occurrence strictly after `now` — repeatedly advances so a reminder
/// delivered late doesn't immediately fire again. Terminates because every
/// recurrence pattern strictly increases `next` while `now` stays fixed.
pub fn next_after(
    current: DateTime<Utc>,
    recurrence: &str,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let mut next = next_occurrence(current, recurrence)?;
    while next <= now {
        next = next_occurrence(next, recurrence)?;
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, mo: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, 9, 0, 0).unwrap()
    }

    #[test]
    fn advances_daily_weekly_monthly() {
        assert_eq!(next_occurrence(utc(2026, 6, 11), "daily"), Some(utc(2026, 6, 12)));
        assert_eq!(next_occurrence(utc(2026, 6, 11), "weekly"), Some(utc(2026, 6, 18)));
        assert_eq!(next_occurrence(utc(2026, 6, 11), "monthly"), Some(utc(2026, 7, 11)));
    }

    #[test]
    fn monthly_clamps_to_month_end() {
        // Jan 31 + 1 month clamps to Feb 28 (2026 is not a leap year).
        assert_eq!(next_occurrence(utc(2026, 1, 31), "monthly"), Some(utc(2026, 2, 28)));
    }

    #[test]
    fn monthly_clamp_does_not_restore_the_original_day() {
        // Pinned trade-off: once clamped (Jan 31 -> Feb 28), later months keep
        // the clamped day rather than returning to the 31st.
        let feb = next_occurrence(utc(2026, 1, 31), "monthly").unwrap();
        assert_eq!(next_occurrence(feb, "monthly"), Some(utc(2026, 3, 28)));
    }

    #[test]
    fn one_shot_has_no_next() {
        assert_eq!(next_occurrence(utc(2026, 6, 11), "none"), None);
        assert_eq!(next_occurrence(utc(2026, 6, 11), "yearly"), None);
    }

    #[test]
    fn next_after_skips_past_occurrences() {
        // A daily reminder delivered 3 days late schedules for tomorrow,
        // not for a time still in the past.
        let next = next_after(utc(2026, 6, 8), "daily", utc(2026, 6, 11)).unwrap();
        assert_eq!(next, utc(2026, 6, 12));
    }
}
