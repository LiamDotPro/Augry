---
id: T16
epic: 001-trigger-core
title: Test a source firing through the hub to a subscriber
status: done
depends_on: [T15]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-hub/src/tests.rs
---

## Goal

Tests prove the whole path: a started `FakeSource` fires an event, it passes the cool down, and a subscriber receives the `Fired`. Another test pins that `Hub` is `Send + Sync`, which the app needs to keep it in Tauri state. No library code changes.

## Read first

- `src-tauri/crates/trigger-hub/src/tests.rs` the file you append to (T10 to T15)
- `src-tauri/crates/trigger-core/src/testkit.rs` `FakeSource::fire`: sends to the sink given to the last `start`; does nothing if never started

## Steps

1. Append this module to the end of `src-tauri/crates/trigger-hub/src/tests.rs` (blank line before it). Do not change any other file.

```rust
mod end_to_end_tests {
    use crate::test_support::shared;
    use crate::Hub;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::{Duration, UNIX_EPOCH};
    use trigger_core::testkit::{rule, FakeSource};
    use trigger_core::Event;

    const WAIT: Duration = Duration::from_secs(5);

    fn event(rule: &str, secs: u64) -> Event {
        Event {
            rule: rule.to_string(),
            kind: "fake.x".to_string(),
            at: UNIX_EPOCH + Duration::from_secs(secs),
            payload: BTreeMap::new(),
        }
    }

    #[test]
    fn hub_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Hub>();
    }

    #[test]
    fn an_event_from_a_started_source_reaches_a_subscriber() {
        let (fake, fake_ref, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let rx = hub.subscribe();
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        fake_ref.fire(event("r1", 100));
        let fired = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(fired.event.rule, "r1");
        assert_eq!(fired.event.kind, "fake.x");
        assert_eq!(fired.coalesced, 0);
    }

    #[test]
    fn events_from_a_source_are_cooled_down_and_counted() {
        let (fake, fake_ref, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let rx = hub.subscribe();
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        fake_ref.fire(event("r1", 100));
        fake_ref.fire(event("r1", 100));
        fake_ref.fire(event("r1", 101));
        fake_ref.fire(event("r1", 105));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
        let later = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(later.event.at, UNIX_EPOCH + Duration::from_secs(105));
        assert_eq!(later.coalesced, 2);
    }

    #[test]
    fn a_restarted_source_still_reaches_subscribers() {
        let (fake, fake_ref, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let rx = hub.subscribe();
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        hub.apply(vec![
            rule("r1", "fake.x", json!({})),
            rule("r2", "fake.x", json!({})),
        ]);
        // `fire` uses the sink from the latest start.
        fake_ref.fire(event("r2", 100));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().event.rule, "r2");
    }

    #[test]
    fn source_events_and_published_events_share_one_cool_down() {
        let (fake, fake_ref, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let rx = hub.subscribe();
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        fake_ref.fire(event("r1", 100));
        hub.publish(event("r1", 101));
        hub.publish(event("r1", 110));
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 0);
        assert_eq!(rx.recv_timeout(WAIT).unwrap().coalesced, 1);
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-hub`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub end_to_end_tests` passes (5 tests)
- [ ] `cd src-tauri && cargo test -p trigger-hub` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only `src-tauri/crates/trigger-hub/src/tests.rs`

## Out of scope

- Any change to `lib.rs`, `test_support.rs` or `trigger-core`, even if a test fails (see If stuck).
- Extra tests, sleeps, or raising timeouts.

## If stuck

- If `hub_is_send_and_sync` does not compile, `Hub` has a field that is not `Send + Sync`. Do not change `lib.rs`. Set status blocked and quote the compiler error.
- If an event test hangs or fails, do not change the code. Set status blocked and write the test name and the failure in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (5 end_to_end_tests pass; hub suite 38 pass; clippy and fmt clean; check-crates exit 0 with the accepted tests.rs size warning at 581 lines)
- 2026-10-12 checker: pass
