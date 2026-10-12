use super::*;
use chrono_tz::America::New_York;
use chrono_tz::Tz;
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

pub(super) fn due_of(f: &Fire) -> u64 {
    f.payload["due"].as_u64().unwrap()
}

pub(super) fn run(state: &mut State, now: SystemTime, rules: &[Armed]) -> Vec<Fire> {
    step::<Tz>(state, &New_York, now, rules)
}

/// Steps every 30 seconds from `from` (exclusive) to `to` (inclusive) and
/// returns every fire.
pub(super) fn walk(
    state: &mut State,
    from: SystemTime,
    to: SystemTime,
    rules: &[Armed],
) -> Vec<Fire> {
    let mut all = Vec::new();
    let mut now = from + secs(30);
    while now <= to {
        all.extend(run(state, now, rules));
        now += secs(30);
    }
    all
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

#[test]
fn a_schedule_fires_once_when_its_minute_arrives() {
    let start = at(2026, 3, 6, 8, 59, 40);
    let rules = [armed("a", "clock.schedule", json!({ "cron": "0 9 * * *" }))];
    let mut state = State::new(ClockConfig::default(), start);
    assert!(run(&mut state, start + secs(10), &rules).is_empty());
    let fired = run(&mut state, at(2026, 3, 6, 9, 0, 5), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].rule, "a");
    assert_eq!(fired[0].kind, "clock.schedule");
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 6, 9, 0, 0)));
    assert_eq!(fired[0].payload["caught_up"], json!(false));
    assert!(run(&mut state, at(2026, 3, 6, 9, 0, 35), &rules).is_empty());
}

#[test]
fn an_interval_fires_every_period_without_drifting() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 60 }))];
    let mut state = State::new(ClockConfig::default(), start);
    // Ticks 30 s apart, each a little late: still one fire per minute, and
    // every due time is exactly 60 s after the last.
    let fired = walk(&mut state, start, start + secs(185), &rules);
    let dues: Vec<u64> = fired.iter().map(due_of).collect();
    let first = millis(start + secs(60));
    assert_eq!(dues, vec![first, first + 60_000, first + 120_000]);
}

#[test]
fn several_slots_in_one_normal_step_each_fire() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 10 }))];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = run(&mut state, start + secs(30), &rules);
    assert_eq!(fired.len(), 3);
    assert!(fired.iter().all(|f| f.payload["caught_up"] == json!(false)));
}

#[test]
fn wait_is_the_time_to_the_next_due_but_never_more_than_a_tick() {
    let start = at(2026, 3, 6, 8, 0, 0);
    let cfg = ClockConfig::default();
    let rules = [armed("a", "clock.schedule", json!({ "cron": "0 9 * * *" }))];
    let mut state = State::new(cfg, start);
    assert_eq!(wait(&state, start), cfg.tick, "nothing planned yet");
    run(&mut state, start + secs(30), &rules);
    assert_eq!(wait(&state, start + secs(30)), cfg.tick, "an hour away");
    let near = at(2026, 3, 6, 8, 59, 50);
    run(&mut state, near, &rules);
    assert_eq!(wait(&state, near), secs(10));
    assert_eq!(wait(&state, at(2026, 3, 6, 9, 5, 0)), Duration::ZERO);
}

#[test]
fn kinds_that_are_not_schedules_are_left_alone() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(ClockConfig::default(), start);
    run(&mut state, start + secs(30), &rules);
    assert!(state.cursors.is_empty());
}
