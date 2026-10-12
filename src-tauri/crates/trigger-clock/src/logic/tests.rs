use super::*;
use chrono_tz::America::New_York;
use serde_json::json;
use trigger_core::testkit::rule;
/// A wall clock time in New York. Only for times that are not ambiguous.
pub(super) fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> SystemTime {
    New_York
        .with_ymd_and_hms(y, mo, d, h, mi, s)
        .unwrap()
        .into()
}

pub(super) fn armed(id: &str, kind: &str, when: serde_json::Value) -> Armed {
    Armed {
        id: id.into(),
        kind: kind.into(),
        parsed: crate::rule::parse(&rule(id, kind, when)).unwrap(),
    }
}

pub(super) fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn next_due_for_a_schedule_is_the_next_matching_minute() {
    let rule = armed("a", "clock.schedule", json!({ "cron": "*/15 * * * *" }));
    let next = next_due(&rule, &New_York, at(2026, 3, 6, 8, 0, 0)).unwrap();
    assert_eq!(next, at(2026, 3, 6, 8, 15, 0));
    // Strictly after: asking from a due time gives the following one.
    let then = next_due(&rule, &New_York, next).unwrap();
    assert_eq!(then, at(2026, 3, 6, 8, 30, 0));
}

#[test]
fn next_due_for_an_interval_adds_the_period() {
    let rule = armed("i", "clock.interval", json!({ "every_s": 90 }));
    let after = at(2026, 3, 6, 8, 0, 0);
    assert_eq!(next_due(&rule, &New_York, after), Some(after + secs(90)));
}

#[test]
fn next_due_is_none_for_other_kinds() {
    let rule = armed("d", "clock.daily-first-use", json!({}));
    assert_eq!(next_due(&rule, &New_York, at(2026, 3, 6, 8, 0, 0)), None);
}

#[test]
fn due_slots_lists_everything_up_to_now_and_the_next_one() {
    let rule = armed("i", "clock.interval", json!({ "every_s": 10 }));
    let start = at(2026, 3, 6, 8, 0, 0);
    let (slots, next) = due_slots(&rule, &New_York, start + secs(10), start + secs(35));
    assert_eq!(
        slots,
        vec![start + secs(10), start + secs(20), start + secs(30)]
    );
    assert_eq!(next, Some(start + secs(40)));
    let (none, next) = due_slots(&rule, &New_York, start + secs(10), start + secs(5));
    assert!(none.is_empty());
    assert_eq!(next, Some(start + secs(10)));
}
