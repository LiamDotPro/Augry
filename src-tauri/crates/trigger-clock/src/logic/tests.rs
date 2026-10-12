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

#[test]
fn a_gap_of_exactly_jump_is_still_normal() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 30 }))];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = run(&mut state, start + secs(90), &rules);
    assert_eq!(fired.len(), 3);
}

#[test]
fn after_a_six_hour_jump_catch_up_fires_once_for_the_latest_missed_time() {
    let start = at(2026, 3, 6, 22, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "0 * * * *", "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(120), &rules);
    // Six hours pass in one step. Hourly slots 23:00 to 04:00 were missed.
    let fired = run(&mut state, start + secs(120 + 6 * 3600), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 4, 0, 0)));
    assert_eq!(fired[0].payload["caught_up"], json!(true));
    // Back to normal: the next slot fires on time and is not a catch up.
    let woke = start + secs(120 + 6 * 3600);
    let fired = walk(&mut state, woke, at(2026, 3, 7, 5, 1, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 5, 0, 0)));
    assert_eq!(fired[0].payload["caught_up"], json!(false));
}

#[test]
fn after_a_jump_without_catch_up_missed_slots_are_skipped() {
    let start = at(2026, 3, 6, 22, 0, 0);
    let rules = [armed("a", "clock.schedule", json!({ "cron": "0 * * * *" }))];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(120), &rules);
    let woke = start + secs(120 + 6 * 3600);
    assert!(run(&mut state, woke, &rules).is_empty());
    // The cursor moved past the missed slots: 05:00 is next, nothing is owed.
    assert!(run(&mut state, woke + secs(30), &rules).is_empty());
    let fired = walk(&mut state, woke, at(2026, 3, 7, 5, 1, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 5, 0, 0)));
}

#[test]
fn a_jump_with_nothing_missed_fires_nothing_even_with_catch_up() {
    let start = at(2026, 3, 6, 8, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "0 9 * * *", "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    // A 10 minute nap that ends before 09:00.
    assert!(run(&mut state, start + secs(60 + 600), &rules).is_empty());
}

#[test]
fn an_interval_with_catch_up_fires_once_after_a_jump() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed(
        "i",
        "clock.interval",
        json!({ "every_s": 3600, "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    // 5.5 hours later: slots at +1 h to +5 h were missed.
    let fired = run(&mut state, start + secs(60 + 19_800), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(start + secs(5 * 3600)));
    assert_eq!(fired[0].payload["caught_up"], json!(true));
    // The next slot keeps the original rhythm: +6 h.
    let woke = start + secs(60 + 19_800);
    let fired = walk(&mut state, woke, start + secs(6 * 3600 + 5), &rules);
    assert_eq!(due_of(&fired[0]), millis(start + secs(6 * 3600)));
}

#[test]
fn an_interval_without_catch_up_skips_the_missed_slots() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 3600 }))];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    assert!(run(&mut state, start + secs(60 + 19_800), &rules).is_empty());
}

#[test]
fn a_backwards_jump_fires_nothing_and_plans_again() {
    let start = at(2026, 3, 6, 9, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "30 9 * * *" }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(600), &rules);
    // The clock is set back an hour, to 08:10.
    let back = at(2026, 3, 6, 8, 10, 0);
    assert!(run(&mut state, back, &rules).is_empty());
    assert_eq!(state.prev_wall, back);
    assert_eq!(state.normal_since, back);
    // 09:30 comes round again and the rule fires for it.
    let fired = walk(&mut state, back, at(2026, 3, 6, 9, 31, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 6, 9, 30, 0)));
}
