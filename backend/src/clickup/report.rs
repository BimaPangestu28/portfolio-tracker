//! Pure helpers for time-tracking input/output: duration parsing, hours
//! aggregation, and period windows. No I/O — fully unit-tested.

use crate::clickup::client::TimeEntry;
use chrono::{DateTime, Datelike, Utc};

/// Parse a human duration into milliseconds. Accepts forms like "2 jam",
/// "90 menit", "1j30m", "1.5 jam", "45m", "2h", "30 min". Returns None when no
/// number+unit pair is found or a unit is unrecognised.
pub fn parse_duration(raw: &str) -> Option<i64> {
    let s = raw.trim().to_lowercase();
    let bytes = s.as_bytes();
    let mut total_minutes: f64 = 0.0;
    let mut matched = false;
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && !bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let num_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        let num: f64 = s[num_start..i].parse().ok()?;
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = &s[unit_start..i];
        let minutes = if unit.starts_with('j') || unit.starts_with('h') {
            num * 60.0
        } else if unit.starts_with('m') {
            num
        } else {
            return None;
        };
        total_minutes += minutes;
        matched = true;
    }
    if !matched {
        return None;
    }
    Some((total_minutes * 60_000.0).round() as i64)
}

/// Render a millisecond duration as "6j 30m" / "1j" / "30m".
pub fn format_duration(ms: i64) -> String {
    let total_minutes = ms / 60_000;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 && minutes > 0 {
        format!("{hours}j {minutes}m")
    } else if hours > 0 {
        format!("{hours}j")
    } else {
        format!("{minutes}m")
    }
}

/// Hours for one project, broken down by task. First-seen order is preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectHours {
    pub project: String,
    pub total_ms: i64,
    pub tasks: Vec<(String, i64)>,
}

/// Group entries by project → task, summing durations. Returns the per-project
/// breakdown and the grand total in ms.
pub fn aggregate_hours(entries: &[TimeEntry]) -> (Vec<ProjectHours>, i64) {
    let mut projects: Vec<ProjectHours> = Vec::new();
    let mut grand_total = 0i64;
    for entry in entries {
        grand_total += entry.duration_ms;
        let project = match projects.iter_mut().find(|p| p.project == entry.project_name) {
            Some(existing) => existing,
            None => {
                projects.push(ProjectHours {
                    project: entry.project_name.clone(),
                    total_ms: 0,
                    tasks: Vec::new(),
                });
                projects.last_mut().expect("just pushed")
            }
        };
        project.total_ms += entry.duration_ms;
        match project.tasks.iter_mut().find(|(name, _)| *name == entry.task_name) {
            Some((_, ms)) => *ms += entry.duration_ms,
            None => project.tasks.push((entry.task_name.clone(), entry.duration_ms)),
        }
    }
    (projects, grand_total)
}

/// UTC [start_ms, end_ms] for a reporting scope over the WIB calendar.
/// "today" = start of today; "week" = Monday this week; "month" = the 1st.
/// Anything else falls back to "week". end is `now`.
pub fn period_window(scope: &str, now_utc: DateTime<Utc>) -> (i64, i64) {
    let wib = crate::assistant::time::wib();
    let now_wib = now_utc.with_timezone(&wib);
    let today = now_wib.date_naive();
    let start_date = match scope {
        "today" => today,
        "month" => today.with_day(1).expect("day 1 is valid"),
        _ => today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64),
    };
    let start_ms = start_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(wib)
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&Utc)
        .timestamp_millis();
    (start_ms, now_utc.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, task: &str, ms: i64) -> TimeEntry {
        TimeEntry {
            task_id: format!("id_{task}"),
            task_name: task.into(),
            project_name: project.into(),
            duration_ms: ms,
            start_ms: 0,
            billable: false,
        }
    }

    #[test]
    fn parse_duration_handles_common_forms() {
        assert_eq!(parse_duration("2 jam"), Some(7_200_000));
        assert_eq!(parse_duration("90 menit"), Some(5_400_000));
        assert_eq!(parse_duration("1j30m"), Some(5_400_000));
        assert_eq!(parse_duration("1.5 jam"), Some(5_400_000));
        assert_eq!(parse_duration("45m"), Some(2_700_000));
        assert_eq!(parse_duration("2h"), Some(7_200_000));
        assert_eq!(parse_duration("banana"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn format_duration_renders_hours_and_minutes() {
        assert_eq!(format_duration(9_000_000), "2j 30m");
        assert_eq!(format_duration(3_600_000), "1j");
        assert_eq!(format_duration(1_800_000), "30m");
    }

    #[test]
    fn aggregate_hours_groups_by_project_and_task() {
        let entries = vec![
            entry("PT AIS", "landing", 4 * 3_600_000),
            entry("PT AIS", "kontrak", 2 * 3_600_000 + 1_800_000),
            entry("PT AIS", "landing", 3_600_000), // same task again
            entry("Klien B", "revisi", 2 * 3_600_000),
        ];
        let (projects, grand) = aggregate_hours(&entries);
        assert_eq!(grand, 4 * 3_600_000 + 2 * 3_600_000 + 1_800_000 + 3_600_000 + 2 * 3_600_000);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project, "PT AIS");
        assert_eq!(projects[0].tasks.len(), 2); // landing merged
        assert_eq!(projects[0].tasks[0], ("landing".to_string(), 5 * 3_600_000));
        assert_eq!(projects[1].project, "Klien B");
    }

    #[test]
    fn period_window_today_starts_at_wib_midnight() {
        // 2026-06-12T05:00:00Z == 12:00 WIB Friday.
        let now = DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (start, end) = period_window("today", now);
        // Start of WIB day = 2026-06-11T17:00:00Z.
        let expected_start = DateTime::parse_from_rfc3339("2026-06-11T17:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected_start);
        assert_eq!(end, now.timestamp_millis());
    }

    #[test]
    fn period_window_week_starts_monday_wib() {
        // Friday 2026-06-12 → Monday is 2026-06-08, 00:00 WIB = 2026-06-07T17:00:00Z.
        let now = DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (start, _) = period_window("week", now);
        let expected = DateTime::parse_from_rfc3339("2026-06-07T17:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected);
    }
}
