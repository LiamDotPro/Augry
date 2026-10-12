---
id: T12
epic: 003-trigger-git
title: Add the Repo trait and a fake for tests
status: todo
depends_on: [T11]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/repo.rs
  - src-tauri/crates/trigger-git/src/logic/fake.rs
---

## Goal

The trait `Repo` (Decision 12) exists in `logic/repo.rs`, with `FakeRepo` in `logic/fake.rs` for tests, so the glue in slice C can be tested without git.

## Read first

- `plans/003-trigger-git/epic.md` Decision 12 (the trait, word for word)
- `src-tauri/crates/trigger-git/src/logic.rs:1-40` the module doc, `#![allow(dead_code)]` and `RepoSnapshot`
- `src-tauri/crates/trigger-git/src/logic.rs` the last lines (`#[cfg(test)]` and `mod tests;`)

## Steps

1. Create `src-tauri/crates/trigger-git/src/logic/repo.rs` with exactly this content. The trait is in `logic` and not in `git.rs` so the glue and its tests never need the git module.

```rust
//! The questions the glue asks about a repository, behind a trait so the
//! glue can be tested with a fake and without git. `GitRepo` in `git.rs` is
//! the real one (Decision 12 of plans/003-trigger-git/epic.md).

use super::RepoSnapshot;

pub trait Repo: Send {
    /// What the repository looks like now. Err holds a short reason, such as
    /// "not a git repository".
    fn snapshot(&self) -> Result<RepoSnapshot, String>;

    /// Commits the current branch is behind its upstream, counted from the
    /// remote refs on disk, after a `git fetch` first when `fetch` is true.
    /// None when there is no upstream (or HEAD is detached, or git failed).
    fn behind(&self, fetch: bool) -> Option<u32>;

    /// Age in whole seconds of the oldest modified tracked file in the work
    /// tree. None when nothing tracked is modified (or git failed).
    fn oldest_change_secs(&self) -> Option<u64>;
}
```

2. Create `src-tauri/crates/trigger-git/src/logic/fake.rs` with exactly this content. `FakeRepo` clones share one state, so a test keeps one handle to change what the repository says while the code under test owns another. Its tests are inline because the whole file only exists in test builds.

```rust
//! A `Repo` for tests. Clones share their state, so a test can keep one
//! handle to change what the repository says while the code under test owns
//! another.

use std::sync::{Arc, Mutex};

use super::repo::Repo;
use super::RepoSnapshot;

#[derive(Default)]
struct State {
    snapshot: Option<Result<RepoSnapshot, String>>,
    behind: Option<u32>,
    age: Option<u64>,
    snapshot_calls: u32,
    fetch_calls: u32,
    plain_behind_calls: u32,
}

#[derive(Clone, Default)]
pub struct FakeRepo {
    state: Arc<Mutex<State>>,
}

impl FakeRepo {
    /// A repository whose snapshot is `snapshot`, with no upstream and no
    /// modified files.
    pub fn new(snapshot: RepoSnapshot) -> FakeRepo {
        let repo = FakeRepo::default();
        repo.set_snapshot(Ok(snapshot));
        repo
    }

    pub fn set_snapshot(&self, snapshot: Result<RepoSnapshot, String>) {
        self.state.lock().unwrap().snapshot = Some(snapshot);
    }

    pub fn set_behind(&self, behind: Option<u32>) {
        self.state.lock().unwrap().behind = behind;
    }

    pub fn set_age(&self, age: Option<u64>) {
        self.state.lock().unwrap().age = age;
    }

    pub fn snapshot_calls(&self) -> u32 {
        self.state.lock().unwrap().snapshot_calls
    }

    /// How many times `behind(true)` was called.
    pub fn fetch_calls(&self) -> u32 {
        self.state.lock().unwrap().fetch_calls
    }

    /// How many times `behind(false)` was called.
    pub fn plain_behind_calls(&self) -> u32 {
        self.state.lock().unwrap().plain_behind_calls
    }
}

impl Repo for FakeRepo {
    fn snapshot(&self) -> Result<RepoSnapshot, String> {
        let mut state = self.state.lock().unwrap();
        state.snapshot_calls += 1;
        state
            .snapshot
            .clone()
            .unwrap_or_else(|| Err("no snapshot set".into()))
    }

    fn behind(&self, fetch: bool) -> Option<u32> {
        let mut state = self.state.lock().unwrap();
        if fetch {
            state.fetch_calls += 1;
        } else {
            state.plain_behind_calls += 1;
        }
        state.behind
    }

    fn oldest_change_secs(&self) -> Option<u64> {
        self.state.lock().unwrap().age
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn a_boxed_repo_can_move_to_a_thread() {
        assert_send::<Box<dyn Repo>>();
        assert_send::<FakeRepo>();
    }

    #[test]
    fn answers_what_it_was_told_and_counts_calls() {
        let snap = RepoSnapshot {
            branch: Some("main".into()),
            ..RepoSnapshot::default()
        };
        let fake = FakeRepo::new(snap.clone());
        let repo: Box<dyn Repo> = Box::new(fake.clone());

        assert_eq!(repo.snapshot(), Ok(snap));
        assert_eq!(repo.behind(false), None);
        assert_eq!(repo.oldest_change_secs(), None);

        fake.set_behind(Some(2));
        fake.set_age(Some(90));
        assert_eq!(repo.behind(true), Some(2));
        assert_eq!(repo.oldest_change_secs(), Some(90));
        assert_eq!(
            (
                fake.snapshot_calls(),
                fake.fetch_calls(),
                fake.plain_behind_calls()
            ),
            (1, 1, 1)
        );
    }

    #[test]
    fn a_snapshot_can_be_made_to_fail() {
        let fake = FakeRepo::default();
        assert_eq!(fake.snapshot(), Err("no snapshot set".to_string()));
        fake.set_snapshot(Err("not a git repository".into()));
        assert_eq!(fake.snapshot(), Err("not a git repository".to_string()));
    }
}
```

3. In `src-tauri/crates/trigger-git/src/logic.rs`, add this at the very end of the file, after a blank line, below the existing `#[cfg(test)]` and `mod tests;` lines.

```rust

pub mod repo;

#[cfg(test)]
pub mod fake;
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 54 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git logic::fake` runs 3 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Implementing `Repo` for anything but `FakeRepo`. `GitRepo` gets its impl in T16.
- Changing the three method signatures, adding methods, or adding a `Sync` bound.
- Using `FakeRepo` from `lib.rs` (slice C does).
- Removing `#![allow(dead_code)]` from `logic.rs`.
- Adding `thiserror` or any dependency.

## If stuck

- If `src/logic/repo.rs` or `src/logic/fake.rs` already exists, set status blocked and say so.
- If the compiler says `RepoSnapshot` is not found in `super`, `logic.rs` does not define it yet (T04 is not done). Set status blocked.
- If clippy reports `dead_code` or `unused`, the line `#![allow(dead_code)]` at the top of `logic.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
