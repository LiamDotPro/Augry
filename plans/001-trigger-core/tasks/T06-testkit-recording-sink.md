---
id: T06
epic: 001-trigger-core
title: Add the testkit module with RecordingSink and rule()
status: todo
depends_on: [T05]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
  - src-tauri/crates/trigger-core/src/testkit.rs
---

## Goal

`trigger_core::testkit` exists with `RecordingSink` (collects events, waits for one by kind) and the `rule()` helper, tested.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs:1-12` where `pub mod testkit;` goes
- `plans/001-trigger-core/epic.md` the section "Shape", `testkit.rs` block

## Steps

1. In `lib.rs`, add `pub mod testkit;` on its own line directly under the `//!` doc comment, above the `use std::collections::BTreeMap;` line, with a blank line after it.

2. Create `src-tauri/crates/trigger-core/src/testkit.rs` with this content:

```rust
//! Helpers for testing sources and the hub. Small enough to live in the
//! contract crate, so every source crate can use them as `trigger_core::testkit`.

use crate::{Event, Rule, Sink, Value};
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Collects the events a source sends. Give `tx.clone()` to `Source::start`.
pub struct RecordingSink {
    pub tx: Sink,
    rx: Receiver<Event>,
    // Events seen while waiting for a different kind. They are kept, so a
    // later `wait_for` or `drain` still sees them.
    skipped: Mutex<Vec<Event>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        RecordingSink { tx, rx, skipped: Mutex::new(Vec::new()) }
    }

    /// Returns the first event of this kind, oldest first, waiting up to
    /// `timeout` for it. Events of other kinds that arrive meanwhile are kept
    /// for later calls. `None` when the time runs out.
    pub fn wait_for(&self, kind: &str, timeout: Duration) -> Option<Event> {
        let mut skipped = self.skipped.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = skipped.iter().position(|e| e.kind == kind) {
            return Some(skipped.remove(i));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(e) if e.kind == kind => return Some(e),
                Ok(e) => skipped.push(e),
                Err(_) => return None,
            }
        }
    }

    /// Everything received and not yet returned, oldest first, without waiting.
    pub fn drain(&self) -> Vec<Event> {
        let mut skipped = self.skipped.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = std::mem::take(&mut *skipped);
        all.extend(self.rx.try_iter());
        all
    }
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a rule. `when` should be a JSON object; anything else gives an empty `when`.
pub fn rule(id: &str, kind: &str, when: Value) -> Rule {
    let when: BTreeMap<String, Value> = match when {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    Rule { id: id.to_string(), kind: kind.to_string(), when }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use serde_json::json;
    use std::time::SystemTime;

    fn event(kind: &str) -> Event {
        Event { rule: "r".into(), kind: kind.into(), at: SystemTime::now(), payload: BTreeMap::new() }
    }

    #[test]
    fn wait_for_returns_a_matching_event() {
        let sink = RecordingSink::new();
        sink.tx.send(event("a")).unwrap();
        let got = sink.wait_for("a", Duration::from_millis(500)).unwrap();
        assert_eq!(got.kind, "a");
    }

    #[test]
    fn wait_for_times_out_with_none() {
        let sink = RecordingSink::new();
        assert!(sink.wait_for("a", Duration::from_millis(50)).is_none());
    }

    #[test]
    fn other_kinds_are_kept_for_drain() {
        let sink = RecordingSink::new();
        sink.tx.send(event("a")).unwrap();
        sink.tx.send(event("b")).unwrap();
        assert!(sink.wait_for("b", Duration::from_millis(500)).is_some());
        let rest = sink.drain();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].kind, "a");
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn rule_builds_from_a_json_object() {
        let r = rule("r1", "fs.x", json!({"path": "/tmp"}));
        assert_eq!(r.id, "r1");
        assert_eq!(r.kind, "fs.x");
        assert_eq!(r.when["path"], json!("/tmp"));
        assert!(rule("r2", "k", json!(null)).when.is_empty());
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core recording` passes (4 tests)
- [ ] `cd src-tauri && cargo test -p trigger-core` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `Clock` and `FakeSource` (T07).
- A cargo feature for the test kit. It is always compiled, by design.
- Adding methods to `RecordingSink` beyond `new`, `wait_for`, `drain`.

## If stuck

- If `unwrap_or_else(|e| e.into_inner())` does not compile on the lock, set status blocked and quote the error.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
