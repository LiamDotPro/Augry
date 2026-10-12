---
id: T08
epic: 007-trigger-clock
title: Pin the daylight saving behaviour with tests
status: todo
depends_on: [T07]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/dst_tests.rs
---

## Goal

A second test file proves the DST cases found in R01 against `next_due` and `step`, in the America/New_York zone through `chrono-tz`, so the tests do not depend on the machine's zone.

## Read first

- `plans/007-trigger-clock/research/R01-cron.md` the Evidence block with the observed spring forward and fall back times
- `plans/007-trigger-clock/epic.md` Decision 8 (cron in local time)
- `src-tauri/crates/trigger-clock/src/logic/tests.rs` the `pub(super)` helpers the new file imports

## Steps

1. In `src-tauri/crates/trigger-clock/src/logic.rs`, find the two lines at the end of the file:

```
#[cfg(test)]
mod tests;
```

Replace them with these four lines:

```
#[cfg(test)]
mod dst_tests;
#[cfg(test)]
mod tests;
```

2. Create `src-tauri/crates/trigger-clock/src/logic/dst_tests.rs` with this content:

```rust
use super::tests::{armed, at, due_of, secs, walk};
use super::*;
use chrono_tz::America::New_York;
use serde_json::json;

/// An instant given in UTC, for the repeated hour of a DST change.
fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> SystemTime {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap().into()
}

/// An instant as New York local time with its zone name, such as
/// `2026-11-01 01:00 EST`.
fn local(t: SystemTime) -> String {
    in_zone(&New_York, t)
        .format("%Y-%m-%d %H:%M %Z")
        .to_string()
}

fn due_times(cron: &str, from: SystemTime, count: usize) -> Vec<String> {
    let rule = armed("a", "clock.schedule", json!({ "cron": cron }));
    let mut out = Vec::new();
    let mut after = from;
    while out.len() < count {
        after = next_due(&rule, &New_York, after).unwrap();
        out.push(local(after));
    }
    out
}

#[test]
fn a_nine_oclock_rule_stays_at_nine_local_across_spring_forward() {
    let from = at(2026, 3, 6, 8, 0, 0);
    assert_eq!(
        due_times("0 9 * * 1-5", from, 3),
        [
            "2026-03-06 09:00 EST",
            "2026-03-09 09:00 EDT",
            "2026-03-10 09:00 EDT"
        ]
    );
}

#[test]
fn a_time_skipped_by_spring_forward_runs_at_the_first_valid_instant() {
    let from = at(2026, 3, 7, 23, 50, 0);
    assert_eq!(
        due_times("30 2 * * *", from, 2),
        ["2026-03-08 03:00 EDT", "2026-03-09 02:30 EDT"]
    );
}

#[test]
fn a_half_hourly_rule_skips_the_missing_hour_on_spring_forward() {
    let from = at(2026, 3, 8, 1, 0, 0);
    assert_eq!(
        due_times("*/30 * * * *", from, 4),
        [
            "2026-03-08 01:30 EST",
            "2026-03-08 03:00 EDT",
            "2026-03-08 03:30 EDT",
            "2026-03-08 04:00 EDT"
        ]
    );
}

#[test]
fn a_fixed_time_in_the_repeated_hour_runs_once() {
    let from = at(2026, 10, 31, 23, 50, 0);
    assert_eq!(
        due_times("30 1 * * *", from, 2),
        ["2026-11-01 01:30 EDT", "2026-11-02 01:30 EST"]
    );
}

#[test]
fn an_hourly_rule_fires_twice_in_the_repeated_hour() {
    let from = utc(2026, 11, 1, 4, 30, 0); // 00:30 EDT
    assert_eq!(
        due_times("0 * * * *", from, 4),
        [
            "2026-11-01 01:00 EDT",
            "2026-11-01 01:00 EST",
            "2026-11-01 02:00 EST",
            "2026-11-01 03:00 EST"
        ]
    );
}

#[test]
fn step_fires_an_hourly_rule_in_both_repeated_hours() {
    let start = utc(2026, 11, 1, 4, 30, 0);
    let rules = [armed("a", "clock.schedule", json!({ "cron": "0 * * * *" }))];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = walk(&mut state, start, start + secs(3 * 3600), &rules);
    let times: Vec<String> = fired
        .iter()
        .map(|f| local(UNIX_EPOCH + Duration::from_millis(due_of(f))))
        .collect();
    assert_eq!(
        times,
        [
            "2026-11-01 01:00 EDT",
            "2026-11-01 01:00 EST",
            "2026-11-01 02:00 EST"
        ]
    );
}

#[test]
fn step_fires_a_skipped_spring_time_at_three() {
    let start = at(2026, 3, 8, 1, 50, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "30 2 * * *" }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = walk(&mut state, start, at(2026, 3, 8, 3, 10, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(
        local(UNIX_EPOCH + Duration::from_millis(due_of(&fired[0]))),
        "2026-03-08 03:00 EDT"
    );
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 38 tests in the library (7 new, in `logic::dst_tests`), 1 in `readme`, 1 ignored in `live`
- [ ] `cd src-tauri && cargo test -p trigger-clock logic::dst_tests::an_hourly_rule_fires_twice_in_the_repeated_hour` passes
- [ ] `wc -l src-tauri/crates/trigger-clock/src/logic.rs src-tauri/crates/trigger-clock/src/logic/tests.rs` shows each under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Changing any non test code to make a DST test pass.
- Deduplicating the double firing of an hourly rule in the repeated hour: the epic accepts it (Decision 8).
- Tests in other zones.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If a DST test fails with a different local time from the one in the test, the `croner` version resolved differently from 4.0.1. Run `cd src-tauri && cargo tree -p trigger-clock -i croner`, write the version in the Log, set `status: blocked` and stop. Do not edit the expected values.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
