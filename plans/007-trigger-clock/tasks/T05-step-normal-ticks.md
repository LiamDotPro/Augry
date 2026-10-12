---
id: T05
epic: 007-trigger-clock
title: Add step and wait for schedules and intervals on normal ticks
status: done
depends_on: [T04]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/tests.rs
---

## Goal

`step(&mut State, tz, now, rules)` fires schedule and interval rules when their time arrives on ordinary 30 second ticks, with `due` and `caught_up: false` in the payload, and `wait` says how long the thread may sleep. Jumps and daily first use come in the next cards.

## Read first

- `plans/007-trigger-clock/research/R03-sleep-and-wake.md` Implications 2 and 3 (the shape of `step`)
- `src-tauri/crates/trigger-clock/src/logic.rs` the file you add to; read `next_due` and `due_slots`
- `src-tauri/crates/trigger-clock/src/logic/tests.rs` the helpers `at`, `armed` and `secs`

## Steps

1. In `src-tauri/crates/trigger-clock/src/logic.rs`, find the two lines at the end of the file:

```
#[cfg(test)]
mod tests;
```

Paste this text directly above them, with a blank line before and after. The cursor of a rule is planned lazily from `prev_wall` the first time the rule is seen, so a rule that appears later is planned from the last tick. `step` here ignores gaps; card T06 changes `fire_due` and `step`.

```rust
/// Schedule and interval rules.
fn fire_due<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    for armed in rules {
        if !matches!(armed.kind.as_str(), "clock.schedule" | "clock.interval") {
            continue;
        }
        let cursor = *state
            .cursors
            .entry(armed.id.clone())
            .or_insert_with(|| next_due(armed, tz, state.prev_wall));
        let Some(cursor) = cursor else { continue };
        let (slots, next) = due_slots(armed, tz, cursor, now);
        state.cursors.insert(armed.id.clone(), next);
        for due in slots {
            out.push(Fire {
                rule: armed.id.clone(),
                kind: armed.kind.clone(),
                payload: due_payload(due, false),
            });
        }
    }
}

/// The rules due at `now`. `tz` is the zone cron expressions and dates are
/// read in: `&chrono::Local` in the app, a fixed zone in tests.
pub fn step<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
) -> Vec<Fire> {
    let mut out = Vec::new();
    fire_due(state, tz, now, rules, &mut out);
    state.prev_wall = now;
    out
}

/// How long the thread may wait before the next step: until the earliest
/// planned due time, but never longer than `cfg.tick`.
pub fn wait(state: &State, now: SystemTime) -> Duration {
    state
        .cursors
        .values()
        .flatten()
        .map(|t| t.duration_since(now).unwrap_or(Duration::ZERO))
        .min()
        .map_or(state.cfg.tick, |d| d.min(state.cfg.tick))
}
```

2. Replace the whole of `src-tauri/crates/trigger-clock/src/logic/tests.rs` with this. It has the same helpers as before plus `due_of`, `run` and `walk`, and five new tests.

```rust
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
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 18 tests in the library (5 new, in `logic::tests`), 1 in `readme`, 1 ignored in `live`
- [ ] `cd src-tauri && cargo test -p trigger-clock logic::tests::an_interval_fires_every_period_without_drifting` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Gap, jump and backwards handling: T06.
- `daily-first-use`: T07.
- A `Clock` trait object or any `Instant` in `logic.rs`: `now` is an argument.
- Editing `lib.rs`.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
