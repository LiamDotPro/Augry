---
id: T22
epic: 003-trigger-git
title: Add the pure settle timer
status: done
depends_on: [T19]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic/settle.rs
  - src-tauri/crates/trigger-git/src/logic.rs
---

## Goal

`logic/settle.rs` has `Settle`, the pure rule for waiting 300 ms after the last file event before taking a snapshot, and pushing the snapshot back (up to 10 times) while `index.lock` or `HEAD.lock` exists.

## Read first

- `plans/003-trigger-git/epic.md` Decision 6 (the last sentence: settle 300 ms, re-arm up to 10 times)
- `plans/003-trigger-git/research/R01-git-files.md` "Settle"
- `src-tauri/crates/trigger-git/src/logic.rs` the last lines, where modules are declared

## Steps

1. Check that `src-tauri/crates/trigger-git/src/logic/settle.rs` does not exist.

2. Create `src-tauri/crates/trigger-git/src/logic/settle.rs` with exactly this content. Time comes in as an argument and the lock check as a function, so nothing here reads a clock or the disk.

```rust
//! The wait between the last file event and the snapshot. Git writes a ref by
//! creating `<ref>.lock` and renaming it, so one commit is a burst of events;
//! this merges the burst into one snapshot. Pure: the caller passes the time
//! and says whether a lock file exists. Decision 6 of
//! plans/003-trigger-git/epic.md.

use std::time::{Duration, Instant};

/// Quiet time after the last event before the snapshot is taken.
pub const SETTLE: Duration = Duration::from_millis(300);

/// How many times a held lock may push the snapshot back by `SETTLE`.
pub const MAX_LOCK_RETRIES: u32 = 10;

#[derive(Debug, Default)]
pub struct Settle {
    deadline: Option<Instant>,
    retries: u32,
}

impl Settle {
    pub fn new() -> Settle {
        Settle::default()
    }

    /// A relevant file event happened at `now`: the wait starts again, and the
    /// lock retries start again with it.
    pub fn touch(&mut self, now: Instant) {
        self.deadline = Some(now + SETTLE);
        self.retries = 0;
    }

    /// When `due` next needs to be called. None when nothing is waiting.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// True when the snapshot should be taken now. Before the deadline, or
    /// with nothing waiting, it is false. At the deadline, if `locked()` says
    /// git is still writing, the wait is pushed back by `SETTLE`, up to
    /// `MAX_LOCK_RETRIES` times; after that the snapshot is taken anyway.
    /// `locked` is only called when the deadline has passed.
    pub fn due(&mut self, now: Instant, locked: impl FnOnce() -> bool) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        if self.retries < MAX_LOCK_RETRIES && locked() {
            self.retries += 1;
            self.deadline = Some(now + SETTLE);
            return false;
        }
        self.deadline = None;
        self.retries = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nothing_is_due_until_something_happens() {
        let mut settle = Settle::new();
        let t0 = Instant::now();
        assert_eq!(settle.deadline(), None);
        assert!(!settle.due(t0 + ms(10_000), || panic!("locked must not be asked")));
    }

    #[test]
    fn it_is_due_after_the_quiet_time_and_only_once() {
        let mut settle = Settle::new();
        let t0 = Instant::now();
        settle.touch(t0);
        assert_eq!(settle.deadline(), Some(t0 + SETTLE));
        assert!(!settle.due(t0 + ms(299), || panic!("too early")));
        assert!(settle.due(t0 + ms(300), || false));
        assert_eq!(settle.deadline(), None);
        assert!(!settle.due(t0 + ms(900), || false));
    }

    #[test]
    fn a_new_event_restarts_the_wait() {
        let mut settle = Settle::new();
        let t0 = Instant::now();
        settle.touch(t0);
        settle.touch(t0 + ms(200));
        assert!(!settle.due(t0 + ms(400), || false));
        assert!(settle.due(t0 + ms(500), || false));
    }

    #[test]
    fn a_held_lock_pushes_the_snapshot_back() {
        let mut settle = Settle::new();
        let t0 = Instant::now();
        settle.touch(t0);
        assert!(!settle.due(t0 + ms(300), || true));
        assert_eq!(settle.deadline(), Some(t0 + ms(600)));
        assert!(settle.due(t0 + ms(600), || false));
    }

    #[test]
    fn a_lock_that_never_goes_away_gives_up_after_ten_retries() {
        let mut settle = Settle::new();
        let mut now = Instant::now();
        settle.touch(now);
        for _ in 0..MAX_LOCK_RETRIES {
            now += SETTLE;
            assert!(!settle.due(now, || true));
        }
        now += SETTLE;
        // The eleventh time the snapshot is taken without asking about locks.
        assert!(settle.due(now, || panic!("no more retries")));
        assert_eq!(settle.deadline(), None);
    }

    #[test]
    fn a_new_event_gives_the_retries_back() {
        let mut settle = Settle::new();
        let mut now = Instant::now();
        settle.touch(now);
        for _ in 0..MAX_LOCK_RETRIES {
            now += SETTLE;
            assert!(!settle.due(now, || true));
        }
        settle.touch(now);
        now += SETTLE;
        assert!(!settle.due(now, || true));
    }
}
```

3. At the very end of `src-tauri/crates/trigger-git/src/logic.rs`, after the line `pub mod polled;`, add a blank line and then this line.

```rust
pub mod settle;
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git logic::settle` runs 6 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Using `Settle` anywhere yet (T23 does).
- Changing `SETTLE` or `MAX_LOCK_RETRIES`. They are Decision 6.
- Removing `#![allow(dead_code)]` from `logic.rs` (T29 does).
- Reading the clock or the disk in this file.
- README changes (T27).

## If stuck

- If `logic.rs` has no line `pub mod polled;`, slice B is not finished. Set status blocked.
- If clippy reports `dead_code` in `settle.rs`, the `#![allow(dead_code)]` at the top of `logic.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Appended `pub mod settle;` at the very end of logic.rs (after `mod polled_tests;`, per "at the very end"). Cargo.lock unchanged.
- 2026-10-12 checker: pass
