---
id: T16
epic: 007-trigger-clock
title: Fire clock.event-starting from step
status: todo
depends_on: [T09]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/events.rs
  - src-tauri/crates/trigger-clock/src/logic/event_tests.rs
---

## Goal

`step` fires `clock.event-starting` for every occurrence in `State::feeds` whose time (the start minus `minutes_before`) has arrived, once per occurrence, honouring the `title` glob. The code lives in a new `logic/events.rs` (`fire_events`), which keeps `logic.rs` under 400 lines. `State` gains `fired_events`, the memory that makes each occurrence fire once.

## Read first

- `plans/007-trigger-clock/epic.md` Decisions 2, 3, 10, 11 (kinds, `minutes_before` default 5, event payload)
- `src-tauri/crates/trigger-clock/src/logic.rs` `State`, `step`, `Occurrence`, `millis`, `Payload`, `Fire` (the pieces you add to)
- `src-tauri/crates/trigger-clock/src/rule.rs` `Parsed::minutes_before()`, `Parsed.title` (a compiled `GlobMatcher`) and `When.feed`
- `src-tauri/crates/trigger-clock/src/logic/tests.rs` the helpers `armed`, `at`, `secs`, `run` that the new test file imports

## Steps

1. Edit `src-tauri/crates/trigger-clock/src/logic.rs` in five places. Each "find" text appears exactly once.

a. Find

```rust
/// Everything `step` remembers between calls.
```

and replace it with

```rust
/// Which occurrence of which event a rule has fired: the rule, the event's
/// UID and its start in milliseconds (an override that moves an occurrence
/// has a new start, so it counts as a new occurrence).
pub type EventKey = (RuleId, String, u64);

/// Everything `step` remembers between calls.
```

b. Find

```rust
    /// Occurrences per feed, as last sent by the feed thread. The event kinds
    /// read this; nothing fills it yet.
    pub feeds: BTreeMap<String, Vec<Occurrence>>,
}
```

and replace it with

```rust
    /// Occurrences per feed, as last sent by the feed thread, keyed by the
    /// `feed` field exactly as the rule wrote it. The event kinds read this.
    pub feeds: BTreeMap<String, Vec<Occurrence>>,
    /// Event occurrences already fired, each with the time after which it can
    /// no longer be due and is forgotten.
    pub fired_events: BTreeMap<EventKey, SystemTime>,
}
```

c. Find

```rust
            feeds: BTreeMap::new(),
        }
```

and replace it with

```rust
            feeds: BTreeMap::new(),
            fired_events: BTreeMap::new(),
        }
```

d. Find the line

```rust
    fire_daily(state, tz, now, rules, &mut out);
```

(inside `step`) and replace it with these two lines

```rust
    fire_daily(state, tz, now, rules, &mut out);
    events::fire_events(state, now, rules, &mut out);
```

e. Find the two lines

```rust
#[cfg(test)]
mod tests;
```

and replace them with

```rust
mod events;

#[cfg(test)]
mod event_tests;

#[cfg(test)]
mod tests;
```

(The `#[cfg(test)] mod dst_tests;` that follows stays as it is.)

2. Create `src-tauri/crates/trigger-clock/src/logic/events.rs` with exactly this content. The rule: an occurrence is due from its moment (here the start minus `minutes_before`) until ten minutes (`EVENT_LATE`) after it, so a step that runs a little late, or just after a wake, still fires it, but a stale event does not. `fired_events` stops a second firing, and entries are dropped once they cannot be due again. Card T17 adds the ended kind to this file.

```rust
//! `clock.event-starting` and `clock.event-ended`: reads the occurrences the
//! feed thread put in `State::feeds` and fires each one once.
//!
//! An occurrence is due from a moment (the start minus `minutes_before`, or
//! the end) until `EVENT_LATE` after it. Inside that window it fires on the
//! first step that sees it and is then remembered, so a step that runs again,
//! or a feed that is sent again, never fires it twice. Outside the window it
//! is skipped: an event that ended an hour ago while the machine slept is not
//! news.

use std::time::{Duration, SystemTime};

use trigger_core::Value;

use super::{millis, Armed, Fire, Payload, State};

/// How long after its due moment an occurrence can still fire.
pub const EVENT_LATE: Duration = Duration::from_secs(600);

fn event_payload(title: &str, start: SystemTime, end: SystemTime, feed: &str) -> Payload {
    Payload::from([
        ("title".to_string(), Value::from(title)),
        ("start".to_string(), Value::from(millis(start))),
        ("end".to_string(), Value::from(millis(end))),
        ("feed".to_string(), Value::from(feed)),
    ])
}

/// Fires the event rules that are due at `now`. `State::feeds` is keyed by the
/// `feed` field exactly as the rule wrote it.
pub(super) fn fire_events(state: &mut State, now: SystemTime, rules: &[Armed], out: &mut Vec<Fire>) {
    // Forget an occurrence once it can no longer be due.
    state.fired_events.retain(|_, forget_at| *forget_at > now);
    for armed in rules {
        let starting = armed.kind == "clock.event-starting";
        if !starting {
            continue;
        }
        let Some(feed) = armed.parsed.when.feed.as_deref() else {
            continue;
        };
        let before = Duration::from_secs(u64::from(armed.parsed.minutes_before()) * 60);
        let mut due = Vec::new();
        for occurrence in state.feeds.get(feed).into_iter().flatten() {
            if let Some(title) = &armed.parsed.title {
                if !title.is_match(&occurrence.title) {
                    continue;
                }
            }
            let Some(at) = occurrence.start.checked_sub(before) else {
                continue;
            };
            // Not yet due, or too late to matter.
            match now.duration_since(at) {
                Ok(age) if age <= EVENT_LATE => due.push((occurrence.clone(), at)),
                _ => {}
            }
        }
        for (occurrence, at) in due {
            let key = (
                armed.id.clone(),
                occurrence.uid.clone(),
                millis(occurrence.start),
            );
            if state.fired_events.contains_key(&key) {
                continue;
            }
            state.fired_events.insert(key, at + EVENT_LATE * 2);
            out.push(Fire {
                rule: armed.id.clone(),
                kind: armed.kind.clone(),
                payload: event_payload(&occurrence.title, occurrence.start, occurrence.end, feed),
            });
        }
    }
}
```

3. Create `src-tauri/crates/trigger-clock/src/logic/event_tests.rs` with exactly this content. Card T17 appends to it.

```rust
//! `clock.event-starting` and `clock.event-ended` in `step`.

use super::tests::{armed, at, run, secs};
use super::*;
use serde_json::json;

const FEED: &str = "/cal.ics";

fn occurrence(uid: &str, title: &str, start: SystemTime, minutes: u64) -> Occurrence {
    Occurrence {
        uid: uid.to_string(),
        title: title.to_string(),
        start,
        end: start + secs(minutes * 60),
    }
}

/// Replaces what the feed thread last sent for `FEED`.
fn send(state: &mut State, list: Vec<Occurrence>) {
    state.feeds.insert(FEED.to_string(), list);
}

fn starting(id: &str, extra: serde_json::Value) -> Armed {
    let mut when = json!({ "feed": FEED });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    armed(id, "clock.event-starting", when)
}

fn standup() -> Occurrence {
    occurrence("standup", "Standup", at(2026, 3, 6, 10, 0, 0), 30)
}

#[test]
fn starting_fires_five_minutes_before_by_default_and_once() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 54, 30), &rules).is_empty());
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].rule, "s");
    assert_eq!(fired[0].kind, "clock.event-starting");
    assert_eq!(fired[0].payload["title"], json!("Standup"));
    assert_eq!(fired[0].payload["feed"], json!(FEED));
    assert_eq!(fired[0].payload.len(), 4);
    assert_eq!(
        fired[0].payload["start"].as_u64().unwrap(),
        millis(at(2026, 3, 6, 10, 0, 0))
    );
    assert_eq!(
        fired[0].payload["end"].as_u64().unwrap(),
        millis(at(2026, 3, 6, 10, 30, 0))
    );
    assert!(run(&mut state, at(2026, 3, 6, 9, 55, 30), &rules).is_empty());
    assert!(run(&mut state, at(2026, 3, 6, 10, 0, 0), &rules).is_empty());
}

#[test]
fn starting_honours_minutes_before() {
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 30, 0));
    send(&mut state, vec![standup()]);
    let early = [starting("s", json!({ "minutes_before": 15 }))];
    assert!(run(&mut state, at(2026, 3, 6, 9, 44, 30), &early).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 45, 0), &early).len(), 1);
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 30, 0));
    send(&mut state, vec![standup()]);
    let on_time = [starting("s", json!({ "minutes_before": 0 }))];
    assert!(run(&mut state, at(2026, 3, 6, 9, 59, 30), &on_time).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 0, 0), &on_time).len(), 1);
}

#[test]
fn a_late_step_still_fires_within_ten_minutes_and_not_after() {
    let rules = [starting("s", json!({}))];
    // Due at 09:55. A step at 10:05:00 is exactly ten minutes late.
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 5, 0), &rules).len(), 1);
    // A step at 10:05:01 is too late, and so is every step after it.
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 5, 1), &rules).is_empty());
    assert!(run(&mut state, at(2026, 3, 6, 10, 5, 31), &rules).is_empty());
}

#[test]
fn waking_from_sleep_fires_an_event_that_became_due_while_asleep() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    run(&mut state, at(2026, 3, 6, 9, 0, 30), &rules);
    // One long jump to 09:58, three minutes after the event became due.
    let fired = run(&mut state, at(2026, 3, 6, 9, 58, 0), &rules);
    assert_eq!(fired.len(), 1);
}

#[test]
fn a_feed_sent_again_does_not_fire_again() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 1);
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 55, 30), &rules).is_empty());
    send(&mut state, vec![standup(), standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 56, 0), &rules).is_empty());
}

#[test]
fn a_feed_that_arrives_after_the_due_time_still_fires() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    assert!(run(&mut state, at(2026, 3, 6, 9, 57, 0), &rules).is_empty());
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 57, 30), &rules).len(), 1);
}

#[test]
fn a_moved_occurrence_fires_again_for_its_new_time() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 1);
    let moved = occurrence("standup", "Standup", at(2026, 3, 6, 11, 0, 0), 30);
    send(&mut state, vec![moved]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 54, 30), &rules).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 55, 0), &rules).len(), 1);
}

#[test]
fn the_title_glob_picks_which_events_fire() {
    let rules = [starting("s", json!({ "title": "stand*" }))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    let retro = occurrence("retro", "Retro", at(2026, 3, 6, 10, 0, 0), 30);
    send(&mut state, vec![retro, standup()]);
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].payload["title"], json!("Standup"));
}

#[test]
fn every_rule_on_a_feed_fires_and_rules_on_other_feeds_do_not() {
    let other = armed(
        "other",
        "clock.event-starting",
        json!({ "feed": "/other.ics" }),
    );
    let rules = [starting("a", json!({})), starting("b", json!({})), other];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    let ids: Vec<&str> = fired.iter().map(|f| f.rule.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
}

#[test]
fn two_events_due_together_both_fire() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    let other = occurrence("other", "Other", at(2026, 3, 6, 10, 0, 0), 15);
    send(&mut state, vec![standup(), other]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 2);
}

#[test]
fn the_memory_of_fired_events_is_forgotten_once_they_cannot_be_due() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(state.fired_events.len(), 1);
    // Due at 09:55, so it can be forgotten after two late windows, 10:15.
    run(&mut state, at(2026, 3, 6, 10, 14, 30), &rules);
    assert_eq!(state.fired_events.len(), 1);
    run(&mut state, at(2026, 3, 6, 10, 15, 30), &rules);
    assert!(state.fired_events.is_empty());
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock logic::event_tests` passes: 11 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `wc -l src-tauri/crates/trigger-clock/src/logic.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- `clock.event-ended`: card T17.
- Filling `State::feeds` from a feed: the feed thread in slice C. Tests put occurrences in directly.
- Changing `fire_due`, `fire_daily`, the jump handling or `wait` (a due event does not shorten the wait; the 30 second tick finds it).
- Changing `EVENT_LATE`, or making it a field of `ClockConfig`.
- Editing `rule.rs`, `lib.rs` or `feed.rs`.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If a "find" text in step 1 is not in `logic.rs` or appears twice, the file is not what slice A's cards produce. Set `status: blocked` and say which one.
- If `use super::tests::{...}` in `event_tests.rs` fails because a helper is private, set `status: blocked` and name it; do not change `tests.rs`.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
