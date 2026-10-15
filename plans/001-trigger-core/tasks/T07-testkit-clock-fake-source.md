---
id: T07
epic: 001-trigger-core
title: Add Clock, FixedClock and FakeSource to the testkit
status: done
depends_on: [T06]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/testkit.rs
---

## Goal

`testkit` also has the `Clock` trait with `SystemClock` and `FixedClock`, and `FakeSource`, a `Source` the hub's tests can start, inspect and fire events from.

## Read first

- `src-tauri/crates/trigger-core/src/testkit.rs` the file you edit (T06)
- `src-tauri/crates/trigger-core/src/lib.rs` the `Source` trait and `Handle`

## Steps

1. Replace the two `use` lines at the top of `testkit.rs` that import from `crate` and `std::sync` so the imports read exactly:

```rust
use crate::{
    Availability, Event, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError, Value,
};
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
```

2. Insert the following after the `rule()` function and before `#[cfg(test)] mod recording_tests`:

```rust
/// A source of "now", so time based logic can be tested without sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Always returns the time inside it. A test changes the time by locking it.
pub struct FixedClock(pub Mutex<SystemTime>);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A source for the hub's tests: records the rules it was started with and
/// fires an event when told to.
pub struct FakeSource {
    pub id: &'static str,
    pub kinds: &'static [KindInfo],
    /// One entry per call to `start`, including calls that fail.
    pub started: Mutex<Vec<Vec<Rule>>>,
    pub availability: Availability,
    /// Set by `start`, used by `fire`.
    pub sink: Mutex<Option<Sink>>,
    /// `start` returns `SourceError::Os` when true.
    pub fail_start: bool,
}

impl FakeSource {
    pub fn new(id: &'static str, kinds: &'static [KindInfo]) -> FakeSource {
        FakeSource {
            id,
            kinds,
            started: Mutex::new(Vec::new()),
            availability: Availability::Available,
            sink: Mutex::new(None),
            fail_start: false,
        }
    }

    /// Sends the event to the sink given to the last `start`. Does nothing if
    /// the source was never started.
    pub fn fire(&self, event: Event) {
        let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = sink.as_ref() {
            let _ = tx.send(event);
        }
    }
}

impl Source for FakeSource {
    fn id(&self) -> &'static str {
        self.id
    }

    fn kinds(&self) -> &'static [KindInfo] {
        self.kinds
    }

    fn availability(&self) -> Availability {
        self.availability.clone()
    }

    fn check(&self, _rule: &Rule) -> Result<(), RuleError> {
        Ok(())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        self.started.lock().unwrap_or_else(|e| e.into_inner()).push(rules);
        if self.fail_start {
            return Err(SourceError::Os("fake start failure".to_string()));
        }
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        Ok(Handle::new(|| {}))
    }
}
```

3. Append this test module at the very end of `testkit.rs`:

```rust
#[cfg(test)]
mod fake_tests {
    use super::*;
    use serde_json::json;
    use std::time::UNIX_EPOCH;

    fn event(kind: &str) -> Event {
        Event { rule: "r".into(), kind: kind.into(), at: UNIX_EPOCH, payload: BTreeMap::new() }
    }

    #[test]
    fn fixed_clock_returns_what_is_inside() {
        let clock = FixedClock(Mutex::new(UNIX_EPOCH));
        assert_eq!(clock.now(), UNIX_EPOCH);
        *clock.0.lock().unwrap() = UNIX_EPOCH + Duration::from_secs(5);
        assert_eq!(clock.now(), UNIX_EPOCH + Duration::from_secs(5));
    }

    #[test]
    fn system_clock_is_close_to_now() {
        let before = SystemTime::now();
        assert!(SystemClock.now() >= before);
    }

    #[test]
    fn fake_source_records_rules_and_fires() {
        let source = FakeSource::new("fake", &[]);
        let sink = RecordingSink::new();
        let rules = vec![rule("r1", "fake.x", json!({}))];
        let _handle = source.start(rules.clone(), sink.tx.clone()).unwrap();
        assert_eq!(source.started.lock().unwrap().as_slice(), &[rules]);
        source.fire(event("fake.x"));
        assert!(sink.wait_for("fake.x", Duration::from_millis(500)).is_some());
    }

    #[test]
    fn fake_source_can_fail_to_start() {
        let mut source = FakeSource::new("fake", &[]);
        source.fail_start = true;
        let sink = RecordingSink::new();
        let result = source.start(vec![], sink.tx.clone());
        assert!(matches!(result, Err(SourceError::Os(_))));
        assert_eq!(source.started.lock().unwrap().len(), 1);
        source.fire(event("fake.x"));
        assert!(sink.drain().is_empty());
    }
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core fake_tests` passes (4 tests)
- [ ] `cd src-tauri && cargo test -p trigger-core` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Constructors or helpers on `FixedClock` beyond the tuple field.
- Making `FakeSource` fire events on its own, count calls to `check`, or stop on `Handle` drop.
- Touching `lib.rs`.

## If stuck

- If clippy reports an unused import in `testkit.rs`, remove only that import and note it in the Log.
- If `FakeSource` fails the `Send + Sync` bound of `Source`, set status blocked and quote the error.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (step 1 replaced the whole import header, lines for crate, collections, sync and time, with the exact block; cargo fmt reflowed nothing else; check-crates warns lib.rs 508 lines, exit 0)
- 2026-10-12 checker: pass
