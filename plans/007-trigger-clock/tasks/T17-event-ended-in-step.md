---
id: T17
epic: 007-trigger-clock
title: Fire clock.event-ended from step
status: done
depends_on: [T16]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic/events.rs
  - src-tauri/crates/trigger-clock/src/logic/event_tests.rs
---

## Goal

`fire_events` also fires `clock.event-ended` when an occurrence's end arrives (not shifted by `minutes_before`), once per occurrence, with the same title filter, lateness window and payload as `clock.event-starting`.

## Read first

- `plans/007-trigger-clock/epic.md` Decisions 2, 10 and 11
- `src-tauri/crates/trigger-clock/src/logic/events.rs` the function you change
- `src-tauri/crates/trigger-clock/src/logic/event_tests.rs` the file you append to, and its helpers `occurrence`, `send`, `starting`, `standup`

## Steps

1. In `src-tauri/crates/trigger-clock/src/logic/events.rs`, find

```rust
        if !starting {
            continue;
        }
```

and replace it with

```rust
        if !starting && armed.kind != "clock.event-ended" {
            continue;
        }
```

2. In the same file, find

```rust
            let Some(at) = occurrence.start.checked_sub(before) else {
                continue;
            };
```

and replace it with

```rust
            let at = if starting {
                occurrence.start.checked_sub(before)
            } else {
                Some(occurrence.end)
            };
            let Some(at) = at else {
                continue;
            };
```

3. In `src-tauri/crates/trigger-clock/src/logic/event_tests.rs`, find the line

```rust
use super::tests::{armed, at, run, secs};
```

and replace it with

```rust
use super::tests::{armed, at, run, secs, walk};
```

4. Append this to the end of `event_tests.rs`, after a blank line (the text below starts with its own blank line, so keep just one). Change nothing above it.

```rust

fn ended(id: &str, extra: serde_json::Value) -> Armed {
    let mut when = json!({ "feed": FEED });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    armed(id, "clock.event-ended", when)
}

#[test]
fn ended_fires_when_the_event_ends_and_once() {
    let rules = [ended("e", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 10, 20, 0));
    send(&mut state, vec![standup()]);
    // Not at the start, and not five minutes before the end.
    assert!(run(&mut state, at(2026, 3, 6, 10, 25, 0), &rules).is_empty());
    assert!(run(&mut state, at(2026, 3, 6, 10, 29, 30), &rules).is_empty());
    let fired = run(&mut state, at(2026, 3, 6, 10, 30, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].rule, "e");
    assert_eq!(fired[0].kind, "clock.event-ended");
    assert_eq!(fired[0].payload["title"], json!("Standup"));
    assert_eq!(fired[0].payload["feed"], json!(FEED));
    assert_eq!(
        fired[0].payload["end"].as_u64().unwrap(),
        millis(at(2026, 3, 6, 10, 30, 0))
    );
    assert!(run(&mut state, at(2026, 3, 6, 10, 30, 30), &rules).is_empty());
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 31, 0), &rules).is_empty());
}

#[test]
fn ended_is_skipped_when_more_than_ten_minutes_late() {
    let rules = [ended("e", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 10, 0, 0));
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 40, 1), &rules).is_empty());
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 10, 0, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 40, 0), &rules).len(), 1);
}

#[test]
fn ended_fires_after_a_sleep_that_ends_soon_after_the_event() {
    let rules = [ended("e", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 10, 20, 0));
    send(&mut state, vec![standup()]);
    run(&mut state, at(2026, 3, 6, 10, 20, 30), &rules);
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 34, 0), &rules).len(), 1);
}

#[test]
fn ended_follows_the_title_glob() {
    let rules = [ended("e", json!({ "title": "retro*" }))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 10, 20, 0));
    let retro = occurrence("retro", "Retro", at(2026, 3, 6, 10, 0, 0), 30);
    send(&mut state, vec![standup(), retro]);
    let fired = run(&mut state, at(2026, 3, 6, 10, 30, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].payload["title"], json!("Retro"));
}

#[test]
fn starting_and_ended_rules_on_one_feed_each_fire_at_their_own_time() {
    let rules = [ended("e", json!({})), starting("s", json!({}))];
    let start = at(2026, 3, 6, 9, 50, 0);
    let mut state = State::new(ClockConfig::default(), start);
    send(&mut state, vec![standup()]);
    let fired = walk(&mut state, start, at(2026, 3, 6, 10, 40, 0), &rules);
    let kinds: Vec<&str> = fired.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["clock.event-starting", "clock.event-ended"]);
}

#[test]
fn a_multi_day_event_ends_once() {
    let rules = [ended("e", json!({})), starting("s", json!({}))];
    let start = at(2026, 3, 6, 9, 50, 0);
    let mut state = State::new(ClockConfig::default(), start);
    // Three days long, ending at 10:00 today. Its start is long past.
    let trip = occurrence("trip", "Trip", at(2026, 3, 3, 10, 0, 0), 3 * 24 * 60);
    send(&mut state, vec![trip]);
    let fired = walk(&mut state, start, at(2026, 3, 6, 11, 30, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, "clock.event-ended");
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock logic::event_tests` passes: 17 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `wc -l src-tauri/crates/trigger-clock/src/logic/event_tests.rs` prints a number under 400
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Any other kind, or a `minutes_after` field.
- Changing `EVENT_LATE` or the `fired_events` memory.
- Editing `logic.rs`: card T16 already wired `fire_events` into `step`.
- README changes: card T18.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If a "find" text is not in the file, T16 was not built as written. Set `status: blocked` and say which one.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
