---
id: T04
epic: 007-trigger-clock
title: Add the clock logic types, next_due and due_slots
status: done
depends_on: [T03]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/tests.rs
---

## Goal

`logic.rs` has `ClockConfig`, `Armed`, `Occurrence`, `State`, `Fire`, `next_due` (the next cron or interval time after an instant, in any chrono time zone) and `due_slots`. `State` already has the field slice B's event kinds will read, so `step` does not need rewriting then.

## Read first

- `plans/007-trigger-clock/epic.md` Decisions 4, 7 and 8 (the loop and cron in local time)
- `plans/007-trigger-clock/research/R03-sleep-and-wake.md` Implications 1 to 5 (State, JUMP, NORMAL_FOR)
- `plans/007-trigger-clock/research/R01-cron.md` the `find_next_occurrence` call and the `TimeZone` generic
- `src-tauri/crates/trigger-clock/src/rule.rs` `Parsed` and `When`, which `Armed` wraps
- `src-tauri/crates/trigger-fs/src/logic.rs:360-375` the `#[cfg(test)] mod tests;` pattern

## Steps

1. Replace the whole of `src-tauri/crates/trigger-clock/src/logic.rs` with this. Time is `SystemTime` everywhere; a zone is only used inside `in_zone` and `next_due`. `next_due(armed, tz, after)` for an interval adds `every_s` to `after`, so for an interval `after` is the previous due time. `State.cursors` holds the next due time per rule and `State.feeds` is for slice B.

```rust
//! Pure decisions: given the wall clock reading, which rules fire and with
//! what payload. No threads, no sleeping, no OS calls: `step` takes `now` as
//! an argument, so a test moves time by passing a different value.
//!
//! The loop around it is designed in the epic's Decisions 4 to 8: the thread
//! wakes at least every `ClockConfig::tick`, reads the wall clock and calls
//! `step`. A gap between two readings of more than `ClockConfig::jump` means
//! the machine slept, because no wait counts time spent asleep.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use trigger_core::{RuleId, Value};

use crate::rule::Parsed;

/// Longest wait between two readings of the wall clock.
pub const TICK: Duration = Duration::from_secs(30);
/// A gap between readings above this means the machine slept (3 ticks).
pub const JUMP: Duration = Duration::from_secs(90);
/// `daily-first-use` fires after the clock has run normally for this long.
pub const NORMAL_FOR: Duration = Duration::from_secs(60);

/// Most due times one rule can fire in a single step. A guard against a
/// pathological rule, not a limit anyone should reach.
const MAX_SLOTS: usize = 10_000;

/// The three durations the loop uses. Tests pass small ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockConfig {
    pub tick: Duration,
    pub jump: Duration,
    pub normal_for: Duration,
}

impl Default for ClockConfig {
    fn default() -> Self {
        ClockConfig {
            tick: TICK,
            jump: JUMP,
            normal_for: NORMAL_FOR,
        }
    }
}

/// A rule this crate is watching for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// One occurrence of a calendar event inside the window the feed thread
/// expanded. Unused until the event kinds are added to `step`.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub title: String,
    pub start: SystemTime,
    pub end: SystemTime,
}

/// Everything `step` remembers between calls.
#[derive(Debug)]
pub struct State {
    pub cfg: ClockConfig,
    /// The wall clock reading of the previous step.
    pub prev_wall: SystemTime,
    /// When the clock last started running normally: process start, or the
    /// end of the last jump or backwards step.
    pub normal_since: SystemTime,
    /// The local date `daily-first-use` last fired for.
    pub last_daily: Option<NaiveDate>,
    /// The next due time of each schedule and interval rule. A rule with no
    /// entry is planned on the next step; an entry of `None` means the rule
    /// will never be due again.
    pub cursors: BTreeMap<RuleId, Option<SystemTime>>,
    /// Occurrences per feed, as last sent by the feed thread. The event kinds
    /// read this; nothing fills it yet.
    pub feeds: BTreeMap<String, Vec<Occurrence>>,
}

impl State {
    /// Starts the clock at `now`: nothing is due, the clock counts as having
    /// run normally since `now`.
    pub fn new(cfg: ClockConfig, now: SystemTime) -> State {
        State {
            cfg,
            prev_wall: now,
            normal_since: now,
            last_daily: None,
            cursors: BTreeMap::new(),
            feeds: BTreeMap::new(),
        }
    }

    /// Forgets every planned due time, so the next step plans from the
    /// current time. Used when the rules change and after a backwards step.
    pub fn replan(&mut self) {
        self.cursors.clear();
    }
}

pub type Payload = BTreeMap<String, Value>;

/// A rule that fired: becomes one `Event`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    pub rule: RuleId,
    pub kind: String,
    pub payload: Payload,
}

pub fn millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn in_zone<Tz: TimeZone>(tz: &Tz, t: SystemTime) -> DateTime<Tz> {
    DateTime::<Utc>::from(t).with_timezone(tz)
}

/// The next due time of a schedule or interval rule strictly after `after`.
/// For a schedule `after` is any instant; for an interval it is the previous
/// due time (or the start). `None` for any other kind, and for a cron
/// expression with no further occurrence.
pub fn next_due<Tz: TimeZone>(armed: &Armed, tz: &Tz, after: SystemTime) -> Option<SystemTime> {
    match armed.kind.as_str() {
        "clock.schedule" => {
            let next = armed
                .parsed
                .cron
                .as_ref()?
                .find_next_occurrence(&in_zone(tz, after), false)
                .ok()?;
            Some(next.into())
        }
        "clock.interval" => Some(after + Duration::from_secs(armed.parsed.when.every_s?)),
        _ => None,
    }
}

/// Every due time from `cursor` up to and including `now`, and the first one
/// after `now`.
fn due_slots<Tz: TimeZone>(
    armed: &Armed,
    tz: &Tz,
    cursor: SystemTime,
    now: SystemTime,
) -> (Vec<SystemTime>, Option<SystemTime>) {
    let mut slots = Vec::new();
    let mut at = Some(cursor);
    while let Some(t) = at.filter(|t| *t <= now) {
        if slots.len() == MAX_SLOTS {
            return (slots, next_due(armed, tz, now));
        }
        slots.push(t);
        at = next_due(armed, tz, t);
    }
    (slots, at)
}

fn due_payload(due: SystemTime, caught_up: bool) -> Payload {
    BTreeMap::from([
        ("due".to_string(), Value::from(millis(due))),
        ("caught_up".to_string(), Value::from(caught_up)),
    ])
}

#[cfg(test)]
mod tests;
```

2. Create the folder `src-tauri/crates/trigger-clock/src/logic/` and the file `src-tauri/crates/trigger-clock/src/logic/tests.rs` with this content. The helpers are `pub(super)` because card T08 adds a second test file that uses them.

```rust
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
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 13 tests in the library (9 in `rule`, 4 in `logic::tests`), 1 in `readme`, 1 ignored in `live`
- [ ] `wc -l src-tauri/crates/trigger-clock/src/logic.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `step`, `wait` and firing: the next cards.
- Changing `lib.rs` or `rule.rs`.
- Removing `#![allow(dead_code)]` from `lib.rs`: slice C does that.
- Using `chrono::Local` anywhere: the zone is always a parameter.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If the compiler says `DateTime<Utc>: From<SystemTime>` is not implemented, `chrono` was built without the `std` feature. Check the `chrono` line in `src-tauri/Cargo.toml` against T01 step 3; if it matches, set `status: blocked` and quote the error.
- If the error is about `croner` (a missing item in `croner::parser`, or `find_next_occurrence` having another signature), the pinned version is not what the card was written against. Run `cd src-tauri && cargo tree -p trigger-clock -i croner` and write the version in the Log. Set `status: blocked` and stop.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
