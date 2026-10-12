---
id: T20
epic: 007-trigger-clock
title: Start the clock thread from ClockSource::start
status: done
depends_on: [T18]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/lib.rs
  - src-tauri/crates/trigger-clock/src/threads.rs
  - src-tauri/crates/trigger-clock/src/threads/tests.rs
---

## Goal

`ClockSource::start` parses the rules and starts the `trigger-clock` thread. The thread waits for `logic::wait`, reads the wall clock, calls `logic::step` with `chrono::Local` and sends each `Fire` as an `Event`. Dropping the handle stops it at once. The glue lives in a new `threads.rs` so `lib.rs` stays small. The feed thread is the next card; the clock thread already accepts the occurrences it will send.

## Read first

- `plans/007-trigger-clock/research/R03-sleep-and-wake.md` Implications 1 and 2 (the loop shape)
- `src-tauri/crates/trigger-clock/src/lib.rs:140-178` `ClockSource` and the old `start` you replace
- `src-tauri/crates/trigger-clock/src/logic.rs` `State::new`, `step`, `wait`, `ClockConfig` (do not edit)
- `src-tauri/crates/trigger-fs/src/lib.rs:164-250` how the reference source starts a thread and stops it
- `src-tauri/crates/trigger-core/src/testkit.rs` `Clock`, `SystemClock`, `FixedClock`, `RecordingSink`

## Steps

1. In `src-tauri/crates/trigger-clock/src/lib.rs`, find the line

```rust
mod rule;
```

and replace it with these two lines:

```rust
mod rule;
mod threads;
```

2. In the same file, find the line `    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {` inside `impl Source for ClockSource`. Delete that line and everything below it to the end of the file, and paste this in its place (it ends the `impl` block, and the file):

```rust
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let mut armed = Vec::new();
        for r in &rules {
            let parsed = rule::parse(r).map_err(|e| SourceError::Os(format!("{}: {e}", r.id)))?;
            armed.push(logic::Armed {
                id: r.id.clone(),
                kind: r.kind.clone(),
                parsed,
            });
        }
        threads::launch(armed, sink, threads::Parts::system())
    }
}
```

3. Create `src-tauri/crates/trigger-clock/src/threads.rs` with exactly this content. Two things to know before you paste. `launch` plans the rules (one `step`) before the thread exists, so a test or caller that moves the clock right after `launch` returns cannot be read as the starting time. The thread waits on one channel, so `Msg::Stop` ends it immediately, however long the wait was.

```rust
//! The threads that join the wall clock and the calendar feeds to the pure
//! logic. Glue only: every decision is made in `logic.rs`, and the OS is
//! reached through `os.rs` and the `Fetcher` trait.
//!
//! `trigger-clock` is the clock thread. It blocks on one channel for as long
//! as `logic::wait` says, then reads the wall clock and calls `logic::step`.
//! The same channel carries the stop message, so stopping is immediate.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::SystemTime;

use trigger_core::testkit::{Clock, SystemClock};
use trigger_core::{Event, Handle, Sink, SourceError};

use crate::logic::{self, Armed, ClockConfig, Occurrence, State};

/// What the clock thread can be told.
pub enum Msg {
    /// Stop the thread.
    Stop,
    /// The feed thread's latest occurrences for one feed, keyed by the `feed`
    /// field exactly as the rules wrote it.
    Feed {
        feed: String,
        occurrences: Vec<Occurrence>,
    },
}

/// Everything the threads take from outside, so a test can replace it.
pub struct Parts {
    pub cfg: ClockConfig,
    pub clock: Arc<dyn Clock>,
}

impl Parts {
    /// The real wall clock and the real timings.
    pub fn system() -> Parts {
        Parts {
            cfg: ClockConfig::default(),
            clock: Arc::new(SystemClock),
        }
    }
}

/// Starts the clock thread for these rules. Dropping the returned handle
/// stops it within a moment: it sends `Msg::Stop` and does not wait.
pub fn launch(armed: Vec<Armed>, sink: Sink, parts: Parts) -> Result<Handle, SourceError> {
    let (tx, rx) = mpsc::channel::<Msg>();
    // Planned here, before the thread exists, so the clock a caller moves
    // after `launch` returns can never be read as the starting time.
    let mut state = State::new(parts.cfg, parts.clock.now());
    step_and_send(&mut state, &armed, &sink, parts.clock.now());
    let clock = parts.clock;
    std::thread::Builder::new()
        .name("trigger-clock".into())
        .spawn(move || clock_loop(state, armed, rx, sink, clock))
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(Handle::new(move || {
        let _ = tx.send(Msg::Stop);
    }))
}

/// Waits, steps and sends what fired, until told to stop or the sink is
/// closed.
fn clock_loop(
    mut state: State,
    armed: Vec<Armed>,
    rx: Receiver<Msg>,
    sink: Sink,
    clock: Arc<dyn Clock>,
) {
    loop {
        match rx.recv_timeout(logic::wait(&state, clock.now())) {
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Msg::Feed { feed, occurrences }) => {
                state.feeds.insert(feed, occurrences);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if !step_and_send(&mut state, &armed, &sink, clock.now()) {
            return;
        }
    }
}

/// One step with the machine's local time zone. Returns false when the sink
/// is closed, which ends the thread.
fn step_and_send(state: &mut State, armed: &[Armed], sink: &Sink, now: SystemTime) -> bool {
    for fire in logic::step(state, &chrono::Local, now, armed) {
        let event = Event {
            rule: fire.rule,
            kind: fire.kind,
            at: now,
            payload: fire.payload,
        };
        if sink.send(event).is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests;
```

4. Create the folder `src-tauri/crates/trigger-clock/src/threads/` and the file `src-tauri/crates/trigger-clock/src/threads/tests.rs` with exactly this content. It moves a `FixedClock` instead of sleeping, and uses a 20 millisecond tick so the thread notices within a blink.

```rust
use super::*;
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde_json::json;
use trigger_core::testkit::{rule, FixedClock, RecordingSink};
use trigger_core::{Rule, Source};

use crate::logic::millis;
use crate::ClockSource;

/// 2026-10-13 12:00 UTC. The tests move a `FixedClock` from here.
fn t0() -> SystemTime {
    Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap().into()
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Short timings so a stopped or moved clock is noticed within 20 ms.
fn short_cfg() -> ClockConfig {
    ClockConfig {
        tick: Duration::from_millis(20),
        jump: secs(90),
        normal_for: secs(60),
    }
}

fn clock_at(t: SystemTime) -> Arc<FixedClock> {
    Arc::new(FixedClock(Mutex::new(t)))
}

fn set(clock: &FixedClock, t: SystemTime) {
    *clock.0.lock().unwrap() = t;
}

fn armed(rules: Vec<Rule>) -> Vec<Armed> {
    rules
        .iter()
        .map(|r| Armed {
            id: r.id.clone(),
            kind: r.kind.clone(),
            parsed: crate::rule::parse(r).unwrap(),
        })
        .collect()
}

fn parts(clock: &Arc<FixedClock>) -> Parts {
    Parts {
        cfg: short_cfg(),
        clock: clock.clone(),
    }
}

#[test]
fn an_interval_fires_when_the_clock_reaches_it() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 10 }))];
    let _handle = launch(armed(rules), sink.tx.clone(), parts(&clock)).unwrap();
    // Nothing is due while the clock stands still.
    assert!(sink
        .wait_for("clock.interval", Duration::from_millis(150))
        .is_none());
    set(&clock, t0() + secs(10));
    let event = sink.wait_for("clock.interval", secs(2)).expect("fired");
    assert_eq!(event.rule, "p/i");
    assert_eq!(event.payload["due"], json!(millis(t0() + secs(10))));
    assert_eq!(event.payload["caught_up"], json!(false));
    assert_eq!(event.at, t0() + secs(10));
}

#[test]
fn a_long_sleep_fires_once_with_catch_up_and_not_at_all_without() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let rules = vec![
        rule(
            "p/with",
            "clock.interval",
            json!({ "every_s": 3600, "catch_up": true }),
        ),
        rule("p/without", "clock.interval", json!({ "every_s": 3600 })),
    ];
    let _handle = launch(armed(rules), sink.tx.clone(), parts(&clock)).unwrap();
    // Six hours pass in one reading: the machine slept.
    set(&clock, t0() + secs(6 * 3600));
    let event = sink.wait_for("clock.interval", secs(2)).expect("fired");
    assert_eq!(event.rule, "p/with");
    assert_eq!(event.payload["caught_up"], json!(true));
    assert_eq!(event.payload["due"], json!(millis(t0() + secs(6 * 3600))));
    std::thread::sleep(Duration::from_millis(150));
    assert!(sink.drain().is_empty());
}

#[test]
fn dropping_the_handle_stops_the_thread_within_a_second() {
    let clock = clock_at(t0());
    let (tx, rx) = channel::<Event>();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 10 }))];
    let handle = launch(armed(rules), tx, parts(&clock)).unwrap();
    drop(handle);
    // The thread owned the only other sender, so it is gone when this reports
    // a disconnect.
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn the_stop_is_not_held_up_by_a_long_tick() {
    let clock = clock_at(t0());
    let (tx, rx) = channel::<Event>();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 3600 }))];
    let slow = Parts {
        cfg: ClockConfig::default(),
        clock: clock.clone(),
    };
    let handle = launch(armed(rules), tx, slow).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn start_names_the_rule_that_does_not_parse() {
    let (tx, _rx) = channel();
    let bad = rule("p/bad", "clock.interval", json!({ "every_s": 1 }));
    match ClockSource.start(vec![bad], tx) {
        Err(SourceError::Os(text)) => assert!(text.starts_with("p/bad: ")),
        other => panic!("expected an Os error, got {:?}", other.err()),
    }
}

#[test]
fn start_runs_with_the_real_clock_and_stops_on_drop() {
    let (tx, rx) = channel();
    let good = rule("p/i", "clock.interval", json!({ "every_s": 3600 }));
    let handle = ClockSource.start(vec![good], tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock threads::` passes: 6 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `grep -c "not implemented yet" src-tauri/crates/trigger-clock/src/lib.rs` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- The feed thread, `Fetcher`, `UreqFetcher` and anything in `feed.rs` or `os.rs`: the next card.
- Removing `#![allow(dead_code)]` from `lib.rs`: card T23. `Msg::Feed` is not sent by anything yet, which is why it stays.
- Changing `logic.rs`, `rule.rs` or any constant in them.
- Joining the thread on stop, or logging from it.
- Editing `README.md`: card T24.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If step 2 cannot find the `start` line, T02 or T11 left `lib.rs` different from what this card expects: set `status: blocked` and quote the lines around `fn start`.
- If a test in `threads::` fails once but passes when run again, the machine was busy: run `cd src-tauri && cargo test -p trigger-clock threads::` three more times. If it fails twice in four runs, set `status: blocked` and quote the failure.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
