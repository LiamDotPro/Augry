---
id: T10
epic: 001-trigger-core
title: Add the trigger-hub crate with its types and catalogue
status: done
depends_on: [T08]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-hub/Cargo.toml
  - src-tauri/crates/trigger-hub/src/lib.rs
  - src-tauri/crates/trigger-hub/src/tests.rs
  - src-tauri/crates/trigger-hub/README.md
  - src-tauri/Cargo.lock
---

## Goal

A `trigger-hub` crate exists in the workspace with `Hub::new`, `Hub::catalogue` and the three public types `SourceInfo`, `Fired` and `RuleState`, tested, including the JSON the frontend will read.

## Read first

- `plans/001-trigger-core/epic.md` section "Shape", the `trigger-hub/src/lib.rs` block
- `src-tauri/crates/trigger-core/src/lib.rs` the types this crate uses (`Source`, `KindInfo`, `Availability`, `Event`)
- `src-tauri/crates/trigger-core/Cargo.toml` the manifest to copy the layout from

## Steps

1. Create `src-tauri/crates/trigger-hub/Cargo.toml` with exactly (written by hand; the trigger crate template is for source crates, not for the hub):

```toml
[package]
name = "trigger-hub"
version = "0.1.0"
edition = "2021"
publish = false
description = "Runs trigger sources: rule registry, apply, cool down and fan out of fired events"

[dependencies]
trigger-core = { path = "../trigger-core" }
serde.workspace = true
serde_json.workspace = true
```

2. Create `src-tauri/crates/trigger-hub/src/lib.rs` with exactly:

```rust
//! The hub: owns the trigger sources, applies a rule set to them, cools down
//! noisy rules and passes fired events to subscribers. See
//! docs/process-triggers.md Part 2.

use trigger_core::{Availability, Event, KindInfo, Source};

#[cfg(test)]
mod tests;

/// Owns the sources. Every method takes `&self`, so the app can keep one in
/// Tauri state.
pub struct Hub {
    sources: Vec<Box<dyn Source>>,
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

impl Hub {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Hub {
        Hub { sources }
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
}
```

3. Create `src-tauri/crates/trigger-hub/src/tests.rs` with exactly:

```rust
//! Tests for the hub. Each card adds its own module below.

mod catalogue_tests {
    use crate::{Fired, Hub, RuleState};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::UNIX_EPOCH;
    use trigger_core::testkit::FakeSource;
    use trigger_core::{Availability, Event, KindInfo, Platforms};

    static KINDS: [KindInfo; 1] = [KindInfo {
        id: "fake.x",
        title: "X",
        summary: "s",
        platforms: Platforms::ALL,
        permission: None,
        when: &[],
        payload: &[],
    }];

    #[test]
    fn catalogue_lists_every_source() {
        let first = FakeSource::new("fake", &KINDS);
        let mut second = FakeSource::new("other", &[]);
        second.availability = Availability::Unsupported {
            reason: "not here".to_string(),
        };
        let hub = Hub::new(vec![Box::new(first), Box::new(second)]);
        let list = hub.catalogue();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "fake");
        assert_eq!(list[0].kinds.len(), 1);
        assert_eq!(list[0].kinds[0].id, "fake.x");
        assert_eq!(list[0].availability, Availability::Available);
        assert_eq!(list[1].id, "other");
        assert!(list[1].kinds.is_empty());
        assert_eq!(
            list[1].availability,
            Availability::Unsupported {
                reason: "not here".to_string()
            }
        );
    }

    #[test]
    fn rule_state_json_is_pinned() {
        assert_eq!(
            serde_json::to_value(RuleState::Active).unwrap(),
            json!({"state": "active"})
        );
        assert_eq!(
            serde_json::to_value(RuleState::Rejected { reason: "r".into() }).unwrap(),
            json!({"state": "rejected", "reason": "r"})
        );
        assert_eq!(
            serde_json::to_value(RuleState::SourceFailed { reason: "r".into() }).unwrap(),
            json!({"state": "sourceFailed", "reason": "r"})
        );
    }

    #[test]
    fn fired_json_is_pinned() {
        let fired = Fired {
            event: Event {
                rule: "r1".into(),
                kind: "fake.x".into(),
                at: UNIX_EPOCH,
                payload: BTreeMap::new(),
            },
            coalesced: 3,
        };
        assert_eq!(
            serde_json::to_value(fired).unwrap(),
            json!({"event": {"rule": "r1", "kind": "fake.x", "at": 0, "payload": {}}, "coalesced": 3})
        );
    }
}
```

4. Create `src-tauri/crates/trigger-hub/README.md` with exactly:

```markdown
# trigger-hub

Runs trigger sources: keeps the rule registry, starts and stops sources so
that exactly the applied rules are live, cools down noisy rules and fans fired
events out to subscribers. Design: `docs/process-triggers.md` Part 2.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-hub`
- `cargo clippy -p trigger-hub --all-targets -- -D warnings`
- `cargo fmt -p trigger-hub --check`

From the repo root: `node plans/_tools/check-crates.mjs`.
```

5. Run `cd src-tauri && cargo fmt -p trigger-hub`, then `cargo test -p trigger-hub`. The glob member `crates/*` picks the crate up with no change to `src-tauri/Cargo.toml`. `Cargo.lock` gains a `trigger-hub` entry; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub` passes (3 tests in `catalogue_tests`)
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:` (the untracked folder counts as `src-tauri/crates/trigger-hub/`)

## Out of scope

- Editing `src-tauri/Cargo.toml`. The glob already covers the new crate.
- The channel, the thread, cool down, `check`, `apply`, `publish` and `subscribe`. Later cards add them.
- Depending on any crate other than `trigger-core`, `serde` and `serde_json`.
- Using `new-trigger-crate.mjs`.

## If stuck

- If the JSON pin for `Fired` fails on the `at` value, check that `trigger_core::Event` has `#[serde(with = "ms_since_epoch")]` on `at`. If it does not, set status blocked.
- If `KindInfo` has different field names from the test, use the names in `trigger-core/src/lib.rs`; do not change `trigger-core`.
- If clippy says `Hub::new` needs a `Default`, it does not (it takes an argument); set status blocked and quote the message.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
