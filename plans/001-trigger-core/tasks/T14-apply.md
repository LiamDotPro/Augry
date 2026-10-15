---
id: T14
epic: 001-trigger-core
title: Add Hub::apply (validate, group, start, restart, stop)
status: done
depends_on: [T13]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-hub/src/lib.rs
  - src-tauri/crates/trigger-hub/src/tests.rs
---

## Goal

`Hub::apply(rules)` starts, restarts or stops sources so that exactly the given rules are live, and returns a `RuleState` for every input rule id. This card writes the whole function; the next card adds the tests for restart, stop and failure behaviour.

## Read first

- `src-tauri/crates/trigger-hub/src/lib.rs` the file you edit (T12, T13)
- `src-tauri/crates/trigger-hub/src/test_support.rs` the `shared()` helper the tests use (T13)
- `src-tauri/crates/trigger-core/src/lib.rs` `Availability`, `Handle`, `Rule`, `RuleId`, `SourceError`

## Steps

1. Edit `src-tauri/crates/trigger-hub/src/lib.rs`.
   a. Add this line directly above the existing `use std::sync::atomic::{AtomicBool, Ordering};` line (keep that line as it is):

```rust
use std::collections::BTreeMap;
```

   and change the `use trigger_core::...` line to exactly:

```rust
use trigger_core::{Availability, Event, Handle, KindInfo, Rule, RuleError, RuleId, Source};
```

   b. Add a field to `Hub`, directly after `sources`:

```rust
    /// What each source is running right now, by source id. Dropping an entry
    /// drops its `Handle`, which stops the source.
    running: Mutex<BTreeMap<&'static str, Running>>,
```

   and in `Hub::new` add `running: Mutex::new(BTreeMap::new()),` directly after `sources,` in the struct literal.

   c. Add these items directly above `impl Hub {` (after the `lock` function):

```rust
/// A started source: the rules it was started with (sorted by id) and its handle.
struct Running {
    rules: Vec<Rule>,
    _handle: Handle,
}

/// Why a source that is not `Available` cannot run, or `None` if it can.
fn unavailable_reason(availability: &Availability) -> Option<String> {
    match availability {
        Availability::Available => None,
        Availability::Unsupported { reason } => Some(reason.clone()),
        Availability::NeedsPermission { how, .. } => Some(format!("needs permission {how}")),
    }
}

fn mark(states: &mut BTreeMap<RuleId, RuleState>, rules: &[Rule], state: RuleState) {
    for rule in rules {
        states.insert(rule.id.clone(), state.clone());
    }
}
```

   d. Inside `impl Hub`, directly after `check` and before `publish`, add:

```rust
    /// Starts, restarts or stops sources so that exactly these rules are live.
    /// Returns the state of every rule that was passed in.
    ///
    /// - A rule no source owns, or that its source's `check` rejects, is
    ///   `Rejected` and goes no further.
    /// - A source that is not `Available` is not started; its rules are
    ///   `SourceFailed`.
    /// - A source whose rules are exactly what it already runs is left alone.
    /// - A source with no rules is stopped. Otherwise the old run is stopped
    ///   and the source is started with the new rules.
    /// - A `start` error fails only that source's rules.
    pub fn apply(&self, rules: Vec<Rule>) -> BTreeMap<RuleId, RuleState> {
        let mut states = BTreeMap::new();
        let mut groups: BTreeMap<usize, Vec<Rule>> = BTreeMap::new();
        for rule in rules {
            let Some(index) = self.source_index(&rule.kind) else {
                let reason = format!("no source for kind {}", rule.kind);
                states.insert(rule.id.clone(), RuleState::Rejected { reason });
                continue;
            };
            match self.sources[index].check(&rule) {
                Ok(()) => groups.entry(index).or_default().push(rule),
                Err(e) => {
                    let reason = e.to_string();
                    states.insert(rule.id.clone(), RuleState::Rejected { reason });
                }
            }
        }

        let mut running = lock(&self.running);
        for (index, source) in self.sources.iter().enumerate() {
            let mut group = groups.remove(&index).unwrap_or_default();
            group.sort_by(|a, b| a.id.cmp(&b.id));
            let id = source.id();

            if group.is_empty() {
                running.remove(id);
                continue;
            }
            if let Some(reason) = unavailable_reason(&source.availability()) {
                running.remove(id);
                mark(&mut states, &group, RuleState::SourceFailed { reason });
                continue;
            }
            if running.get(id).is_some_and(|r| r.rules == group) {
                mark(&mut states, &group, RuleState::Active);
                continue;
            }
            // Stop the old run before starting the new one.
            running.remove(id);
            match source.start(group.clone(), self.tx.clone()) {
                Ok(handle) => {
                    mark(&mut states, &group, RuleState::Active);
                    running.insert(
                        id,
                        Running {
                            rules: group,
                            _handle: handle,
                        },
                    );
                }
                Err(e) => {
                    let reason = e.to_string();
                    mark(&mut states, &group, RuleState::SourceFailed { reason });
                }
            }
        }
        states
    }
```

2. Append this module to the end of `src-tauri/crates/trigger-hub/src/tests.rs` (blank line before it):

```rust
mod apply_tests {
    use crate::test_support::shared;
    use crate::{Hub, RuleState};
    use serde_json::json;
    use trigger_core::testkit::{rule, FakeSource};
    use trigger_core::{Availability, Permission};

    fn ids(rules: &[trigger_core::Rule]) -> Vec<&str> {
        rules.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn a_rule_no_source_owns_is_rejected() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("r1", "other.x", json!({})),
            rule("r2", "plain", json!({})),
        ]);
        assert_eq!(
            states["r1"],
            RuleState::Rejected {
                reason: "no source for kind other.x".to_string()
            }
        );
        assert_eq!(
            states["r2"],
            RuleState::Rejected {
                reason: "no source for kind plain".to_string()
            }
        );
        assert!(started.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_rule_the_source_rejects_is_rejected_and_the_rest_start() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), true);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("bad1", "fake.x", json!({"bad": 1})),
            rule("good", "fake.x", json!({})),
        ]);
        assert_eq!(
            states["bad1"],
            RuleState::Rejected {
                reason: "field bad: not allowed".to_string()
            }
        );
        assert_eq!(states["good"], RuleState::Active);
        let calls = started.started.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(ids(&calls[0]), vec!["good"]);
    }

    #[test]
    fn start_is_called_once_with_the_rules_sorted_by_id() {
        let (fake, started, _) = shared(FakeSource::new("fake", &[]), false);
        let hub = Hub::new(vec![fake]);
        let states = hub.apply(vec![
            rule("r2", "fake.x", json!({})),
            rule("r1", "fake.y", json!({"a": 1})),
        ]);
        assert_eq!(states.len(), 2);
        assert_eq!(states["r1"], RuleState::Active);
        assert_eq!(states["r2"], RuleState::Active);
        let calls = started.started.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(ids(&calls[0]), vec!["r1", "r2"]);
        assert_eq!(calls[0][0].when["a"], json!(1));
    }

    #[test]
    fn each_source_gets_only_its_own_rules() {
        let (a, a_started, _) = shared(FakeSource::new("alpha", &[]), false);
        let (b, b_started, _) = shared(FakeSource::new("beta", &[]), false);
        let hub = Hub::new(vec![a, b]);
        hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
            rule("r3", "alpha.y", json!({})),
        ]);
        assert_eq!(ids(&a_started.started.lock().unwrap()[0]), vec!["r1", "r3"]);
        assert_eq!(ids(&b_started.started.lock().unwrap()[0]), vec!["r2"]);
    }

    #[test]
    fn an_unavailable_source_is_not_started() {
        let mut unsupported = FakeSource::new("alpha", &[]);
        unsupported.availability = Availability::Unsupported {
            reason: "nope".to_string(),
        };
        let mut needs = FakeSource::new("beta", &[]);
        needs.availability = Availability::NeedsPermission {
            permission: Permission::Accessibility,
            how: "Open System Settings".to_string(),
        };
        let (a, a_started, _) = shared(unsupported, false);
        let (b, b_started, _) = shared(needs, false);
        let hub = Hub::new(vec![a, b]);
        let states = hub.apply(vec![
            rule("r1", "alpha.x", json!({})),
            rule("r2", "beta.x", json!({})),
        ]);
        assert_eq!(
            states["r1"],
            RuleState::SourceFailed {
                reason: "nope".to_string()
            }
        );
        assert_eq!(
            states["r2"],
            RuleState::SourceFailed {
                reason: "needs permission Open System Settings".to_string()
            }
        );
        assert!(a_started.started.lock().unwrap().is_empty());
        assert!(b_started.started.lock().unwrap().is_empty());
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-hub`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub apply_tests` passes (5 tests)
- [ ] `cd src-tauri && cargo test -p trigger-hub` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Tests for re-apply, restart and stop. The next card has them.
- Retry timers, a "started at" time, or reporting rule ids that were not passed in.
- Handling two rules with the same id (the later one wins in the returned map; leave it).
- Changing `RuleState` or the order in which sources are visited (the order of `Hub::new`'s vector).
- Editing the README.

## If stuck

- If the compiler says the `let ... else` or `is_some_and` syntax is not supported, run `rustc --version`; both need Rust 1.70 or newer. If older, set status blocked and say so.
- If `running.remove(id)` or `running.get(id)` does not compile because of the key type, the map is `BTreeMap<&'static str, Running>` and `id` is a `&'static str`; pass `id` as is, never `&id`.
- If clippy reports the `rules` field of `Running` as never read, check that the `r.rules == group` line from step 1d is present.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
