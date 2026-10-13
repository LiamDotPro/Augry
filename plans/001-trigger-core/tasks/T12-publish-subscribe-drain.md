---
id: T12
epic: 001-trigger-core
title: Add the hub channel, drain thread, publish and subscribe
status: done
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-hub/src/lib.rs
  - src-tauri/crates/trigger-hub/src/tests.rs
---

## Goal

`Hub` owns one internal channel and a thread named `trigger-hub` that reads it, applies the cool down and sends `Fired` to every subscriber. `publish`, `subscribe` and `with_cool_down` work, and the thread stops when the `Hub` is dropped.

## Read first

- `src-tauri/crates/trigger-hub/src/lib.rs` the file you replace (T10, T11)
- `src-tauri/crates/trigger-hub/src/cool_down.rs` the `CoolDown` you use (T11)
- `plans/001-trigger-core/epic.md` Decision 14 and the Shape block for `trigger-hub`

## Steps

1. Replace the whole of `src-tauri/crates/trigger-hub/src/lib.rs` with exactly this (the types and `catalogue` are unchanged from T10; everything else is new):

```rust
//! The hub: owns the trigger sources, applies a rule set to them, cools down
//! noisy rules and passes fired events to subscribers. See
//! docs/process-triggers.md Part 2.

pub mod cool_down;

use cool_down::CoolDown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;
use trigger_core::{Availability, Event, KindInfo, Source};

#[cfg(test)]
mod tests;

const DEFAULT_COOL_DOWN: Duration = Duration::from_secs(2);

/// How often the drain thread looks at the stop flag when nothing arrives.
const POLL: Duration = Duration::from_millis(50);

/// Owns the sources. Every method takes `&self`, so the app can keep one in
/// Tauri state.
pub struct Hub {
    sources: Vec<Box<dyn Source>>,
    /// Sources get clones of this as their sink, and `publish` sends into it.
    tx: Sender<Event>,
    subscribers: Arc<Mutex<Vec<Sender<Fired>>>>,
    cool_down: Arc<Mutex<CoolDown>>,
    stop: Arc<AtomicBool>,
    /// `None` only if the operating system refused to start the thread.
    thread: Option<JoinHandle<()>>,
}

/// One source as the frontend sees it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: &'static str,
    pub kinds: Vec<KindInfo>,
    pub availability: Availability,
}

/// An event that passed the cool down, as sent to subscribers.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fired {
    pub event: Event,
    /// How many events for the same rule were dropped by the cool down since
    /// the last `Fired` for that rule.
    pub coalesced: u32,
}

/// What happened to one rule in `Hub::apply`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuleState {
    Active,
    Rejected { reason: String },
    SourceFailed { reason: String },
}

/// A poisoned lock only means another thread panicked; the data is still fine.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Hub {
        let (tx, rx) = channel();
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let cool_down = Arc::new(Mutex::new(CoolDown::new(DEFAULT_COOL_DOWN)));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let subscribers = Arc::clone(&subscribers);
            let cool_down = Arc::clone(&cool_down);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("trigger-hub".to_string())
                .spawn(move || drain(rx, subscribers, cool_down, stop))
                .ok()
        };
        Hub {
            sources,
            tx,
            subscribers,
            cool_down,
            stop,
            thread,
        }
    }

    /// Replaces the default two second cool down per rule.
    pub fn with_cool_down(self, d: Duration) -> Hub {
        lock(&self.cool_down).set_window(d);
        self
    }

    /// Every source with its kinds and whether it can run here.
    pub fn catalogue(&self) -> Vec<SourceInfo> {
        self.sources
            .iter()
            .map(|s| SourceInfo {
                id: s.id(),
                kinds: s.kinds().to_vec(),
                availability: s.availability(),
            })
            .collect()
    }

    /// Sends an event into the same channel the sources use, so it goes through
    /// the cool down like any other. For triggers that come from inside the app.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    /// A new receiver of every `Fired` from now on. Drop it to unsubscribe.
    pub fn subscribe(&self) -> Receiver<Fired> {
        let (tx, rx) = channel();
        lock(&self.subscribers).push(tx);
        rx
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The body of the `trigger-hub` thread.
fn drain(
    rx: Receiver<Event>,
    subscribers: Arc<Mutex<Vec<Sender<Fired>>>>,
    cool_down: Arc<Mutex<CoolDown>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match rx.recv_timeout(POLL) {
            Ok(event) => {
                let admitted = lock(&cool_down).admit(&event.rule, event.at);
                if let Some(coalesced) = admitted {
                    let fired = Fired { event, coalesced };
                    // A subscriber whose receiver was dropped fails to send and is removed.
                    lock(&subscribers).retain(|s| s.send(fired.clone()).is_ok());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
```

2. Append this module to the end of `src-tauri/crates/trigger-hub/src/tests.rs` (after the existing `catalogue_tests` module, with a blank line between):

```rust
mod publish_tests {
    use crate::Hub;
    use std::collections::BTreeMap;
    use std::sync::mpsc::Receiver;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use trigger_core::Event;

    use crate::Fired;

    const WAIT: Duration = Duration::from_secs(5);

    fn event(rule: &str, secs: u64) -> Event {
        Event {
            rule: rule.to_string(),
            kind: "fake.x".to_string(),
            at: UNIX_EPOCH + Duration::from_secs(secs),
            payload: BTreeMap::new(),
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn nothing_more(rx: &Receiver<Fired>) -> bool {
        rx.recv_timeout(Duration::from_millis(200)).is_err()
    }

    #[test]
    fn publish_reaches_a_subscriber() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        let fired = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(fired.event.rule, "r1");
        assert_eq!(fired.coalesced, 0);
    }

    #[test]
    fn publish_reaches_every_subscriber() {
        let hub = Hub::new(vec![]);
        let a = hub.subscribe();
        let b = hub.subscribe();
        hub.publish(event("r1", 100));
        assert_eq!(a.recv_timeout(WAIT).unwrap().event.rule, "r1");
        assert_eq!(b.recv_timeout(WAIT).unwrap().event.rule, "r1");
    }

    #[test]
    fn a_dropped_subscriber_does_not_stop_the_others() {
        let hub = Hub::new(vec![]);
        let gone = hub.subscribe();
        let kept = hub.subscribe();
        drop(gone);
        hub.publish(event("r1", 100));
        assert!(kept.recv_timeout(WAIT).is_ok());
        hub.publish(event("r2", 100));
        assert_eq!(kept.recv_timeout(WAIT).unwrap().event.rule, "r2");
    }

    #[test]
    fn cool_down_drops_and_counts_and_the_next_fired_carries_the_count() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r1", 101));
        hub.publish(event("r1", 101));
        hub.publish(event("r1", 103));
        let first = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(first.event.at, at(100));
        assert_eq!(first.coalesced, 0);
        let second = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(second.event.at, at(103));
        assert_eq!(second.coalesced, 2);
        assert!(nothing_more(&rx));
        // The count was reset.
        hub.publish(event("r1", 106));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
    }

    #[test]
    fn rules_cool_down_separately() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r2", 100));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().event.rule, "r1");
        assert_eq!(rx.recv_timeout(WAIT).unwrap().event.rule, "r2");
    }

    #[test]
    fn with_cool_down_replaces_the_default() {
        let hub = Hub::new(vec![]).with_cool_down(Duration::ZERO);
        let rx = hub.subscribe();
        hub.publish(event("r1", 100));
        hub.publish(event("r1", 100));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
    }

    #[test]
    fn dropping_the_hub_stops_the_thread_quickly() {
        let hub = Hub::new(vec![]);
        let rx = hub.subscribe();
        let started = std::time::Instant::now();
        drop(hub);
        assert!(started.elapsed() < Duration::from_secs(2));
        // The thread is gone, so its sender to this subscriber is gone too.
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-hub`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub publish_tests` passes (7 tests)
- [ ] `cd src-tauri && cargo test -p trigger-hub` passes (cool_down, catalogue_tests and publish_tests all ok)
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `check`, `apply` and the running map. Later cards add them.
- Any wall clock in the cool down. It uses `event.at` only.
- Changing the poll interval, adding more threads, or using `unwrap`/`expect` in `lib.rs`.
- Editing the README.

## If stuck

- If clippy says `use` ordering or an unused import in `tests.rs`, fix only that import; do not restructure the modules.
- If `Hub` is reported as not `Send` or `Sync`, quote the error and set status blocked.
- If a test is flaky (fails on timing), do not raise the cool down or add sleeps; set status blocked and quote the failure.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
