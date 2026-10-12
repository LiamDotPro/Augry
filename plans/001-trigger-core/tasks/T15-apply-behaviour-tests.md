---
id: T15
epic: 001-trigger-core
title: Test apply restart, stop and failing start with FakeSource
status: todo
depends_on: [T14]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-hub/src/tests.rs
---

## Goal

Tests prove the rest of `apply`: same rules do not restart, different rules restart, no rules stops, and a failing `start` fails only its own source. No library code changes.

## Read first

- `src-tauri/crates/trigger-hub/src/tests.rs` the file you append to (T10 to T14)
- `src-tauri/crates/trigger-hub/src/test_support.rs` `shared()`: returns the boxed source, the `Arc<FakeSource>` (read `started`) and the stop counter
- `src-tauri/crates/trigger-hub/src/lib.rs` the `apply` method, so you know what each test exercises

## Steps

1. Append this module to the end of `src-tauri/crates/trigger-hub/src/tests.rs` (blank line before it). Do not change any other file.

```rust
mod lifecycle_tests {
    use crate::test_support::shared;
    use crate::{Hub, RuleState};
    use serde_json::json;
    use std::sync::atomic::Ordering;
    use trigger_core::testkit::{rule, FakeSource};

    #[test]
    fn applying_the_same_rules_again_does_not_restart() {
        let (fake, fake_ref, stops) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let first = hub.apply(vec![
            rule("r1", "fake.x", json!({"a": 1})),
            rule("r2", "fake.x", json!({})),
        ]);
        // Same rules, given in the other order.
        let second = hub.apply(vec![
            rule("r2", "fake.x", json!({})),
            rule("r1", "fake.x", json!({"a": 1})),
        ]);
        assert_eq!(first, second);
        assert_eq!(second["r1"], RuleState::Active);
        assert_eq!(fake_ref.started.lock().unwrap().len(), 1);
        assert_eq!(stops.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn adding_a_rule_restarts_the_source() {
        let (fake, fake_ref, stops) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        let states = hub.apply(vec![
            rule("r1", "fake.x", json!({})),
            rule("r2", "fake.x", json!({})),
        ]);
        assert_eq!(states["r2"], RuleState::Active);
        let calls = fake_ref.started.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].len(), 1);
        assert_eq!(calls[1].len(), 2);
        // The first run was stopped before the second started.
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn changing_a_rules_fields_restarts_the_source() {
        let (fake, fake_ref, stops) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        hub.apply(vec![rule("r1", "fake.x", json!({"a": 1}))]);
        hub.apply(vec![rule("r1", "fake.x", json!({"a": 2}))]);
        let calls = fake_ref.started.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1][0].when["a"], json!(2));
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn applying_no_rules_stops_the_source() {
        let (fake, fake_ref, stops) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        let states = hub.apply(vec![]);
        assert!(states.is_empty());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        // Applying nothing again stops nothing more and starts nothing.
        hub.apply(vec![]);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(fake_ref.started.lock().unwrap().len(), 1);
        // Applying a rule again starts the source again.
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        assert_eq!(fake_ref.started.lock().unwrap().len(), 2);
    }

    #[test]
    fn rejecting_every_rule_of_a_source_stops_it() {
        let (fake, _, stops) = shared(FakeSource::new("fake", &[]), true);
        let hub = Hub::new(vec![fake]);
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        let states = hub.apply(vec![rule("r1", "fake.x", json!({"bad": 1}))]);
        assert!(matches!(states["r1"], RuleState::Rejected { .. }));
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_change_for_one_source_leaves_the_other_running() {
        let (a, a_ref, a_stops) = shared(FakeSource::new("alpha", &[]), false);
        let (b, b_ref, b_stops) = shared(FakeSource::new("beta", &[]), false);
        let hub = Hub::new(vec![a, b]);
        hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
        ]);
        hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({"changed": true})),
        ]);
        assert_eq!(a_ref.started.lock().unwrap().len(), 1);
        assert_eq!(a_stops.load(Ordering::SeqCst), 0);
        assert_eq!(b_ref.started.lock().unwrap().len(), 2);
        assert_eq!(b_stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failing_start_fails_only_its_source_and_is_retried() {
        let mut failing = FakeSource::new("alpha", &[]);
        failing.fail_start = true;
        let (a, a_ref, _) = shared(failing, false);
        let (b, b_ref, _) = shared(FakeSource::new("beta", &[]), false);
        let hub = Hub::new(vec![a, b]);
        let rules = vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
        ];
        let states = hub.apply(rules.clone());
        assert_eq!(
            states["r1"],
            RuleState::SourceFailed {
                reason: "fake start failure".to_string()
            }
        );
        assert_eq!(states["r2"], RuleState::Active);
        assert_eq!(a_ref.started.lock().unwrap().len(), 1);
        assert_eq!(b_ref.started.lock().unwrap().len(), 1);
        // Applying the same rules again tries the failed source again and
        // leaves the working one alone.
        hub.apply(rules);
        assert_eq!(a_ref.started.lock().unwrap().len(), 2);
        assert_eq!(b_ref.started.lock().unwrap().len(), 1);
    }

    #[test]
    fn dropping_the_hub_stops_every_source() {
        let (fake, _, stops) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        hub.apply(vec![rule("r1", "fake.x", json!({}))]);
        drop(hub);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }
}
```

2. Run `cd src-tauri && cargo fmt -p trigger-hub`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub lifecycle_tests` passes (8 tests)
- [ ] `cd src-tauri && cargo test -p trigger-hub` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only `src-tauri/crates/trigger-hub/src/tests.rs`

## Out of scope

- Any change to `lib.rs`, `test_support.rs` or `trigger-core`, even if a test fails (see If stuck).
- Extra tests beyond the eight above.

## If stuck

- If a test fails because `apply` behaves differently from the description in its doc comment, do not change `lib.rs`. Set status blocked and write the test name and the assertion that failed in the Log.
- If `stops` is off by one in a drop test, remember a `Handle` counts one stop when it is dropped; do not change the expected numbers, set status blocked instead.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
