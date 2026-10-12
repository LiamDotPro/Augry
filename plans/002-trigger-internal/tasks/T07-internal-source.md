---
id: T07
epic: 002-trigger-internal
title: Add InternalSource, the tg Source
status: todo
depends_on: [T06]
size: M
model: haiku
files:
  - src-tauri/src/triggers/internal/mod.rs
---

## Goal

`InternalSource` implements `trigger_core::Source` with id `tg`. It is cloneable (an `Arc` inside): `start` stores the parsed rules and the sink, dropping the handle clears them, `publish(&signal)` sends an `Event` for every rule `step` returns, and `wants_output()` is true only while a `tg.output-matched` rule is armed.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 10
- `src-tauri/crates/trigger-fs/src/lib.rs:146-185` `FsSource::start` and the `Handle` it returns
- `src-tauri/crates/trigger-core/src/lib.rs:150-200` `Handle`, `Source`, `Sink`, `SourceError`
- `src-tauri/crates/trigger-hub/src/lib.rs:171-230` `Hub::apply`: it drops the old `Handle` before it calls `start` again
- `src-tauri/src/triggers/internal/mod.rs` the file you replace (T05)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. Replace the whole of `src-tauri/src/triggers/internal/mod.rs` with exactly this content. How it works: `Shared` is the state every clone shares. `start` parses every rule (a failure is a `SourceError::Os` that names the rule), stores the rules and the sink, sets the output flag and returns a `Handle` whose stop function clears all three. `publish` runs `logic::step` under the lock, then sends one event per fired rule with the current time. The locks use `unwrap_or_else(|e| e.into_inner())` so a poisoned lock never panics.

```rust
//! The `tg` source: triggers the app raises itself (an agent finished, a
//! command failed, a worktree created). The app has the events, so there is no
//! crate and no OS layer: the feeds call `TriggerService::publish_internal`.
//! It has the same split as a trigger crate: `kinds.rs` (the catalogue),
//! `rule.rs` and `logic.rs` (pure, tested) and this glue. See README.md.

mod kinds;
pub mod logic;
pub mod rule;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use trigger_core::{
    Availability, Event, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError,
};

pub use kinds::KINDS;
use logic::Armed;

/// Something the app saw. The feeds build one and hand it to
/// `TriggerService::publish_internal`.
#[derive(Debug, Clone, PartialEq)]
pub enum InternalSignal {
    AgentFinished {
        pane: String,
        cwd: String,
        agent: String,
    },
    CommandEnded {
        pane: String,
        cwd: String,
        exit_code: i32,
        duration_ms: u64,
    },
    OutputLine {
        pane: String,
        cwd: String,
        line: String,
    },
    Bell {
        pane: String,
    },
    TaskReported {
        pane: String,
        cwd: String,
    },
    PaneOpened {
        pane: String,
        cwd: String,
        repo_root: Option<String>,
    },
    WorktreeCreated {
        repo: String,
        path: String,
        branch: Option<String>,
    },
    WorktreeRemoved {
        repo: String,
        path: String,
    },
    ReviewStaged {
        repo: String,
        files: u32,
    },
    NoteSaved {
        path: String,
        relative: String,
    },
    FocusEntered {
        pane: String,
        cwd: String,
    },
    AppStarted,
}

#[derive(Default)]
struct Shared {
    /// The rules of the current run; empty when no run is live.
    armed: Mutex<Vec<Armed>>,
    sink: Mutex<Option<Sink>>,
    /// True while a `tg.output-matched` rule is armed.
    output: AtomicBool,
}

impl Shared {
    fn clear(&self) {
        lock(&self.armed).clear();
        *lock(&self.sink) = None;
        self.output.store(false, Ordering::Relaxed);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cheap to clone: every clone shares the same state. `sources()` boxes one
/// for the hub and `TriggerService` keeps another to feed it.
#[derive(Clone, Default)]
pub struct InternalSource {
    shared: Arc<Shared>,
}

impl InternalSource {
    /// True while a `tg.output-matched` rule is armed, so the terminal reader
    /// can skip assembling lines the rest of the time.
    pub fn wants_output(&self) -> bool {
        self.shared.output.load(Ordering::Relaxed)
    }

    /// Fires every armed rule the signal matches. Returns how many events were
    /// sent. Does nothing while no run is live.
    pub fn publish(&self, signal: &InternalSignal) -> usize {
        let fires = logic::step(&lock(&self.shared.armed), signal);
        if fires.is_empty() {
            return 0;
        }
        let Some(sink) = lock(&self.shared.sink).clone() else {
            return 0;
        };
        let at = SystemTime::now();
        let mut sent = 0;
        for (rule, kind, payload) in fires {
            let event = Event {
                rule,
                kind,
                at,
                payload,
            };
            if sink.send(event).is_ok() {
                sent += 1;
            }
        }
        sent
    }
}

impl Source for InternalSource {
    fn id(&self) -> &'static str {
        "tg"
    }

    fn kinds(&self) -> &'static [KindInfo] {
        KINDS
    }

    fn availability(&self) -> Availability {
        Availability::Available
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        rule::parse(rule).map(|_| ())
    }

    /// Stores the rules and the sink; the hub stops the run by dropping the
    /// handle, which clears them. Patterns are compiled here, once.
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let mut armed = Vec::new();
        for r in &rules {
            let parsed = rule::parse(r).map_err(|e| SourceError::Os(format!("{}: {e}", r.id)))?;
            armed.push(Armed {
                id: r.id.clone(),
                kind: r.kind.clone(),
                parsed,
            });
        }
        let output = armed.iter().any(|a| a.kind == "tg.output-matched");
        *lock(&self.shared.armed) = armed;
        *lock(&self.shared.sink) = Some(sink);
        self.shared.output.store(output, Ordering::Relaxed);
        let shared = Arc::clone(&self.shared);
        Ok(Handle::new(move || shared.clear()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::{channel, Receiver};
    use trigger_core::testkit::rule;

    fn started(source: &InternalSource, rules: Vec<Rule>) -> (Handle, Receiver<Event>) {
        let (tx, rx) = channel();
        (source.start(rules, tx).unwrap(), rx)
    }

    fn bell() -> InternalSignal {
        InternalSignal::Bell { pane: "p1".into() }
    }

    #[test]
    fn it_is_the_tg_source_with_thirteen_kinds() {
        let source = InternalSource::default();
        assert_eq!(source.id(), "tg");
        assert_eq!(source.kinds().len(), 13);
        assert_eq!(source.availability(), Availability::Available);
    }

    #[test]
    fn check_accepts_good_rules_and_rejects_bad_ones() {
        let source = InternalSource::default();
        assert!(source.check(&rule("a/b", "tg.bell", json!({}))).is_ok());
        assert!(source
            .check(&rule(
                "a/b",
                "tg.output-matched",
                json!({ "pattern": "error" })
            ))
            .is_ok());
        assert!(matches!(
            source.check(&rule("a/b", "tg.nope", json!({}))),
            Err(RuleError::UnknownKind(_))
        ));
        assert!(source
            .check(&rule("a/b", "tg.output-matched", json!({ "pattern": "(" })))
            .is_err());
    }

    #[test]
    fn a_published_signal_reaches_the_sink_as_an_event() {
        let source = InternalSource::default();
        let (_handle, rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert_eq!(source.publish(&bell()), 1);
        let event = rx.try_recv().unwrap();
        assert_eq!(event.rule, "a/b");
        assert_eq!(event.kind, "tg.bell");
        assert_eq!(event.payload["pane"], json!("p1"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_clone_feeds_the_same_run() {
        let source = InternalSource::default();
        let feeder = source.clone();
        let (_handle, rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert_eq!(feeder.publish(&bell()), 1);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn dropping_the_handle_stops_the_run() {
        let source = InternalSource::default();
        let (handle, rx) = started(
            &source,
            vec![rule("a/b", "tg.output-matched", json!({ "pattern": "x" }))],
        );
        assert!(source.wants_output());
        drop(handle);
        assert!(!source.wants_output());
        assert_eq!(source.publish(&bell()), 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn wants_output_only_while_an_output_rule_is_armed() {
        let source = InternalSource::default();
        assert!(!source.wants_output());
        let (handle, _rx) = started(&source, vec![rule("a/b", "tg.bell", json!({}))]);
        assert!(!source.wants_output());
        drop(handle);
        let (_handle, _rx) = started(
            &source,
            vec![rule("a/b", "tg.output-matched", json!({ "pattern": "x" }))],
        );
        assert!(source.wants_output());
    }

    #[test]
    fn start_fails_when_a_rule_does_not_parse() {
        let source = InternalSource::default();
        let (tx, _rx) = channel();
        let result = source.start(vec![rule("a/b", "tg.command-slow", json!({}))], tx);
        assert!(matches!(result, Err(SourceError::Os(_))));
        assert!(!source.wants_output());
    }

    #[test]
    fn publishing_with_no_run_sends_nothing() {
        assert_eq!(InternalSource::default().publish(&bell()), 0);
    }
}
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::internal` passes and reports 43 passed
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Registering the source in `sources()` or any change to `triggers/mod.rs`. T08 does it.
- Threads, timers or a clock type. `publish` runs on the caller's thread.
- Debouncing. The hub's cool down already does that.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `cargo clippy` reports a new warning in `internal/mod.rs`, write it in the Log and set `status: blocked`; do not add `#[allow]`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
