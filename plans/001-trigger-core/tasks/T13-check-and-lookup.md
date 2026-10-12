---
id: T13
epic: 001-trigger-core
title: Add source lookup and Hub::check, with test helpers
status: todo
depends_on: [T12]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-hub/src/lib.rs
  - src-tauri/crates/trigger-hub/src/test_support.rs
  - src-tauri/crates/trigger-hub/src/tests.rs
---

## Goal

`Hub::check(&Rule)` finds the source that owns a rule's kind and asks it to check the rule. The hub's tests get a shared helper, `Shared`, that wraps a `FakeSource` so a test can still read what it recorded after the hub has taken ownership.

## Read first

- `src-tauri/crates/trigger-hub/src/lib.rs` the file you edit (T12)
- `src-tauri/crates/trigger-core/src/testkit.rs` `FakeSource` (T07): its `check` always succeeds and its `Handle` does nothing
- `plans/001-trigger-core/epic.md` Shape, `trigger-hub/src/lib.rs`

## Steps

1. Edit `src-tauri/crates/trigger-hub/src/lib.rs`.
   a. Change the import line `use trigger_core::{Availability, Event, KindInfo, Source};` to:

```rust
use trigger_core::{Availability, Event, KindInfo, Rule, RuleError, Source};
```

   b. Directly under the existing `#[cfg(test)] mod tests;` lines add (a blank line between):

```rust
#[cfg(test)]
// Some helpers are used only by later tests.
#[allow(dead_code)]
mod test_support;
```

   c. Inside `impl Hub`, after `catalogue` and before `publish`, add:

```rust
    /// The index of the source that owns this kind. A kind belongs to the
    /// source whose id is the part before the first dot: `fs.file-saved`
    /// belongs to `fs`.
    fn source_index(&self, kind: &str) -> Option<usize> {
        let prefix = kind.split('.').next().unwrap_or("");
        self.sources.iter().position(|s| s.id() == prefix)
    }

    /// Asks the source that owns the rule's kind to check the rule. A kind no
    /// source owns is `RuleError::UnknownKind`.
    pub fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        match self.source_index(&rule.kind) {
            Some(index) => self.sources[index].check(rule),
            None => Err(RuleError::UnknownKind(rule.kind.clone())),
        }
    }
```

2. Create `src-tauri/crates/trigger-hub/src/test_support.rs` with exactly:

```rust
//! Helpers shared by the hub's tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use trigger_core::testkit::FakeSource;
use trigger_core::{Availability, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError};

/// Wraps a `FakeSource` in an `Arc`, so a test keeps a handle on it after the
/// hub has taken ownership of the box. Also:
/// - `reject_bad`: `check` fails for any rule whose `when` has a key `bad`,
///   with the message `field bad: not allowed`.
/// - `stops`: counts how many times a `Handle` this source returned was dropped.
pub struct Shared {
    pub inner: Arc<FakeSource>,
    pub reject_bad: bool,
    pub stops: Arc<AtomicUsize>,
}

/// Returns the box for `Hub::new`, plus the two things a test reads afterwards.
pub fn shared(
    fake: FakeSource,
    reject_bad: bool,
) -> (Box<dyn Source>, Arc<FakeSource>, Arc<AtomicUsize>) {
    let inner = Arc::new(fake);
    let stops = Arc::new(AtomicUsize::new(0));
    let source = Shared {
        inner: Arc::clone(&inner),
        reject_bad,
        stops: Arc::clone(&stops),
    };
    (Box::new(source), inner, stops)
}

impl Source for Shared {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn kinds(&self) -> &'static [KindInfo] {
        self.inner.kinds()
    }

    fn availability(&self) -> Availability {
        self.inner.availability()
    }

    fn check(&self, rule: &Rule) -> Result<(), RuleError> {
        if self.reject_bad && rule.when.contains_key("bad") {
            return Err(RuleError::Field {
                field: "bad".to_string(),
                problem: "not allowed".to_string(),
            });
        }
        self.inner.check(rule)
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        let handle = self.inner.start(rules, sink)?;
        let stops = Arc::clone(&self.stops);
        Ok(Handle::new(move || {
            drop(handle);
            stops.fetch_add(1, Ordering::SeqCst);
        }))
    }
}
```

3. Append this module to the end of `src-tauri/crates/trigger-hub/src/tests.rs` (blank line before it):

```rust
mod check_tests {
    use crate::test_support::shared;
    use crate::Hub;
    use serde_json::json;
    use trigger_core::testkit::{rule, FakeSource};
    use trigger_core::RuleError;

    fn hub() -> Hub {
        let (fs, _, _) = shared(FakeSource::new("fs", &[]), true);
        let (git, _, _) = shared(FakeSource::new("git", &[]), false);
        Hub::new(vec![fs, git])
    }

    #[test]
    fn a_good_rule_passes_the_owning_source() {
        assert!(hub().check(&rule("r1", "fs.file-saved", json!({}))).is_ok());
        assert!(hub().check(&rule("r2", "git.commit", json!({}))).is_ok());
    }

    #[test]
    fn the_owning_source_can_reject_a_rule() {
        let err = hub()
            .check(&rule("r1", "fs.file-saved", json!({"bad": 1})))
            .unwrap_err();
        assert_eq!(err.to_string(), "field bad: not allowed");
        // The git source in this test does not reject "bad".
        assert!(hub()
            .check(&rule("r2", "git.commit", json!({"bad": 1})))
            .is_ok());
    }

    #[test]
    fn a_kind_no_source_owns_is_unknown() {
        let err = hub().check(&rule("r1", "other.x", json!({}))).unwrap_err();
        assert!(matches!(err, RuleError::UnknownKind(ref k) if k == "other.x"));
    }

    #[test]
    fn only_the_part_before_the_first_dot_picks_the_source() {
        // "fsx.y" must not match the source "fs", and "fs" alone is not "fs.".
        assert!(hub().check(&rule("r1", "fsx.y", json!({}))).is_err());
        assert!(hub().check(&rule("r2", "fs", json!({}))).is_ok());
        assert!(hub().check(&rule("r3", "fs.a.b", json!({}))).is_ok());
    }
}
```

   Note: the test `only_the_part_before_the_first_dot_picks_the_source` expects kind `fs` (no dot) to be owned by source `fs`; that follows from the lookup rule above.

4. Run `cd src-tauri && cargo fmt -p trigger-hub`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-hub check_tests` passes (4 tests)
- [ ] `cd src-tauri && cargo test -p trigger-hub` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `apply`. The next card adds it.
- Making `Hub::check` check that the kind is in the source's `kinds()` list. The source decides that in its own `check`.
- Changing `FakeSource` in `trigger-core`. `Shared` wraps it instead.
- Removing the `#[allow(dead_code)]` on `test_support`; the next cards use the rest of it.

## If stuck

- If `FakeSource::check` or `FakeSource::start` is not found, bring the `Source` trait into scope (`use trigger_core::Source;` is already in `test_support.rs`); do not change `trigger-core`.
- If `Shared` is reported not `Send` or `Sync`, quote the error and set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
