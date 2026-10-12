---
id: T04
epic: 001-trigger-core
title: Add RuleError, SourceError, Handle and the Source trait
status: done
depends_on: [T03]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/src/lib.rs
---

## Goal

The error enums, the stop-on-drop `Handle` and the `Source` trait exist in `trigger-core`, so a source crate can implement the whole contract.

## Read first

- `src-tauri/crates/trigger-core/src/lib.rs` the file you edit (T01 to T03)
- `plans/001-trigger-core/epic.md` the section "Shape", the lib.rs block for trigger-core

## Steps

1. Insert the following after `pub type Sink = Sender<Event>;` and before the first `#[cfg(test)]` module:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("unknown kind {0}")]
    UnknownKind(String),
    #[error("field {field}: {problem}")]
    Field { field: String, problem: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Os(String),
}

/// Dropping it stops the watcher. Stop must not block for more than a second.
pub struct Handle(Option<Box<dyn FnOnce() + Send>>);

impl Handle {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Handle {
        Handle(Some(Box::new(stop)))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop()
        }
    }
}

pub trait Source: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn kinds(&self) -> &'static [KindInfo];
    fn availability(&self) -> Availability;
    fn check(&self, rule: &Rule) -> Result<(), RuleError>;
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError>;
}
```

2. Append this test module at the very end of the file:

```rust
#[cfg(test)]
mod handle_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn dropping_a_handle_runs_stop_once() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let handle = Handle::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(count.load(Ordering::SeqCst), 0);
        drop(handle);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn error_messages() {
        assert_eq!(RuleError::UnknownKind("x.y".into()).to_string(), "unknown kind x.y");
        assert_eq!(
            RuleError::Field { field: "path".into(), problem: "is required".into() }.to_string(),
            "field path: is required"
        );
        assert_eq!(SourceError::Os("boom".into()).to_string(), "boom");
        assert_eq!(SourceError::Unavailable("no".into()).to_string(), "no");
    }
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-core`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-core` passes (including the 2 tests in `handle_tests`)
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `kinds_markdown` (T05) and the test kit (T06, T07).
- Adding variants to the error enums, or a `Debug` impl for `Handle`.
- Changing the `Source` trait signature. The hub and every source crate are written against it.

## If stuck

- If clippy complains, quote the lint in the Log and set status blocked. Do not add `#[allow]`.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
