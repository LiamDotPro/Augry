//! `expand` against the handwritten calendar files in `tests/fixtures/`.
//! Times in the expectations are UTC; the feeds are written in other zones.

use super::*;
use chrono::DateTime;
use chrono_tz::Europe::London;

fn t(iso: &str) -> SystemTime {
    DateTime::parse_from_rfc3339(iso).unwrap().into()
}

/// The occurrences as `(start, end)` in RFC 3339 form, for easy comparison.
fn spans(x: &Expansion) -> Vec<(String, String)> {
    let show = |s: SystemTime| DateTime::<chrono::Utc>::from(s).to_rfc3339();
    x.occurrences
        .iter()
        .map(|o| (show(o.start), show(o.end)))
        .collect()
}

fn span(start: &str, end: &str) -> (String, String) {
    (start.to_string(), end.to_string())
}

fn london(text: &str, from: &str, to: &str) -> Expansion {
    expand(text, CalTz::from(London), t(from), t(to))
}

const WEEKLY: &str = include_str!("../../tests/fixtures/weekly_london.ics");
const EXDATE: &str = include_str!("../../tests/fixtures/exdate_daily.ics");
const MOVED: &str = include_str!("../../tests/fixtures/recurrence_id_move.ics");
const ALL_DAY: &str = include_str!("../../tests/fixtures/all_day.ics");

#[test]
fn a_weekly_london_event_follows_daylight_saving() {
    // Thursday 15 October 2026 is British Summer Time: 09:00 is 08:00 UTC.
    let summer = london(WEEKLY, "2026-10-13T22:00:00Z", "2026-10-15T22:00:00Z");
    assert_eq!(
        spans(&summer),
        vec![span(
            "2026-10-15T08:00:00+00:00",
            "2026-10-15T08:30:00+00:00"
        )]
    );
    assert_eq!(summer.occurrences[0].title, "Standup");
    assert_eq!(summer.occurrences[0].uid, "weekly-standup@fixture");
    // Monday 26 October is after the clocks went back: 09:00 is 09:00 UTC.
    let winter = london(WEEKLY, "2026-10-25T22:00:00Z", "2026-10-26T22:00:00Z");
    assert_eq!(
        spans(&winter),
        vec![span(
            "2026-10-26T09:00:00+00:00",
            "2026-10-26T09:30:00+00:00"
        )]
    );
}

#[test]
fn an_excluded_date_is_missing_from_a_daily_series() {
    let x = london(EXDATE, "2026-10-13T00:00:00Z", "2026-10-16T00:00:00Z");
    assert_eq!(
        spans(&x),
        vec![
            span("2026-10-13T13:00:00+00:00", "2026-10-13T14:00:00+00:00"),
            span("2026-10-15T13:00:00+00:00", "2026-10-15T14:00:00+00:00"),
        ]
    );
}

#[test]
fn a_moved_occurrence_appears_once_with_its_own_title() {
    let x = london(MOVED, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z");
    assert_eq!(
        spans(&x),
        vec![span(
            "2026-10-15T15:00:00+00:00",
            "2026-10-15T16:00:00+00:00"
        )]
    );
    assert_eq!(x.occurrences[0].title, "Sync (moved)");
    // The day before is not moved.
    let before = london(MOVED, "2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z");
    assert_eq!(before.occurrences[0].title, "Sync");
}

#[test]
fn all_day_events_run_midnight_to_midnight_in_the_default_zone() {
    let x = london(ALL_DAY, "2026-10-13T00:00:00Z", "2026-10-15T22:00:00Z");
    // 14 October is excluded. British Summer Time: local midnight is 23:00 UTC.
    assert_eq!(
        spans(&x),
        vec![
            span("2026-10-12T23:00:00+00:00", "2026-10-13T23:00:00+00:00"),
            span("2026-10-14T23:00:00+00:00", "2026-10-15T23:00:00+00:00"),
        ]
    );
    assert_eq!(x.occurrences[0].title, "Out of office");
}

#[test]
fn the_window_keeps_events_that_overlap_it_and_drops_the_rest() {
    // The Review event on 15 October runs 13:00 to 14:00 UTC.
    let count = |from: &str, to: &str| london(EXDATE, from, to).occurrences.len();
    assert_eq!(count("2026-10-15T13:30:00Z", "2026-10-15T13:40:00Z"), 1);
    assert_eq!(count("2026-10-15T14:00:00Z", "2026-10-15T14:00:00Z"), 1);
    assert_eq!(count("2026-10-15T14:00:01Z", "2026-10-15T14:30:00Z"), 0);
    assert_eq!(count("2026-10-15T12:00:00Z", "2026-10-15T13:00:00Z"), 1);
    assert_eq!(count("2026-10-15T12:00:00Z", "2026-10-15T12:59:59Z"), 0);
}
