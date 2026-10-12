---
id: T25
epic: 003-trigger-git
title: Add FolderSlot for git.repo-cloned
status: todo
depends_on: [T24]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/worker/folder.rs
  - src-tauri/crates/trigger-git/src/worker/folder/tests.rs
  - src-tauri/crates/trigger-git/src/worker.rs
---

## Goal

`worker/folder.rs` has `FolderSlot`: one `folder` for `git.repo-cloned`. On its interval it lists the folder (through a function passed in) and returns an `Event` per new repository, using `CloneScan`. `worker.rs` exports it.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`repo-cloned`)
- `src-tauri/crates/trigger-git/src/logic/polled.rs` `CloneScan::observe`, `cloned_payload`
- `src-tauri/crates/trigger-git/src/os.rs` `scan_folder` (the real listing, passed in by T27)
- `src-tauri/crates/trigger-git/src/worker.rs` the file you edit
- `src-tauri/crates/trigger-git/src/worker/repo.rs` the helper `event` that `folder.rs` uses

## Steps

1. Check that `src-tauri/crates/trigger-git/src/worker/folder.rs` does not exist.

2. Create `src-tauri/crates/trigger-git/src/worker/folder.rs` with exactly this content.

```rust
//! One folder watched for `git.repo-cloned`. It has no file watcher: the
//! folder is listed on the rule's interval and `CloneScan` says which
//! children are new. The listing is passed in as a function, so tests do not
//! need a real folder.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use trigger_core::Event;

use super::repo::event;
use crate::logic::polled::{cloned_payload, CloneScan};

pub struct FolderSlot {
    id: String,
    folder: PathBuf,
    interval: Duration,
    next_poll: Instant,
    scan: CloneScan,
    /// Names of the children of a folder that hold a repository.
    list: fn(&Path) -> BTreeSet<String>,
}

impl FolderSlot {
    /// The first `tick` is due at once and becomes the baseline.
    pub fn new(
        id: String,
        folder: PathBuf,
        interval_s: u64,
        now: Instant,
        list: fn(&Path) -> BTreeSet<String>,
    ) -> FolderSlot {
        FolderSlot {
            id,
            folder,
            interval: Duration::from_secs(interval_s),
            next_poll: now,
            scan: CloneScan::new(),
            list,
        }
    }

    /// When `tick` next has something to do.
    pub fn deadline(&self) -> Instant {
        self.next_poll
    }

    /// Lists the folder if it is due and returns one event per new repository.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        if now < self.next_poll {
            return Vec::new();
        }
        self.next_poll = now + self.interval;
        let names = (self.list)(&self.folder);
        self.scan
            .observe(names)
            .iter()
            .map(|name| event(&self.id, "git.repo-cloned", cloned_payload(&self.folder, name)))
            .collect()
    }
}

#[cfg(test)]
mod tests;
```

3. Create the folder `src-tauri/crates/trigger-git/src/worker/folder/` and in it `tests.rs` with exactly this content. The fake listing is a `thread_local`, because the listing function is a plain `fn` pointer and cannot capture anything.

```rust
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

thread_local! {
    /// What the fake listing returns. Each test runs on its own thread, so
    /// tests do not see each other's value.
    static LISTING: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

fn list(_folder: &Path) -> BTreeSet<String> {
    LISTING.with(|l| l.borrow().clone())
}

fn set_listing(names: &[&str]) {
    LISTING.with(|l| *l.borrow_mut() = names.iter().map(|n| n.to_string()).collect());
}

fn slot(now: Instant) -> FolderSlot {
    FolderSlot::new("p/c".into(), PathBuf::from("/work"), 30, now, list)
}

#[test]
fn the_first_listing_is_a_baseline() {
    set_listing(&["app", "lib"]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert_eq!(slot.deadline(), t0);
    assert!(slot.tick(t0).is_empty());
    assert_eq!(slot.deadline(), t0 + Duration::from_secs(30));
}

#[test]
fn a_new_child_fires_with_its_path() {
    set_listing(&["app"]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert!(slot.tick(t0).is_empty());

    set_listing(&["app", "tool"]);
    let events = slot.tick(t0 + Duration::from_secs(30));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].rule, "p/c");
    assert_eq!(events[0].kind, "git.repo-cloned");
    let expected = PathBuf::from("/work").join("tool");
    assert_eq!(events[0].payload["path"], json!(expected.to_string_lossy()));
    assert_eq!(events[0].payload.len(), 1);

    // The same child is not reported twice.
    assert!(slot.tick(t0 + Duration::from_secs(60)).is_empty());
}

#[test]
fn nothing_is_listed_before_the_interval_has_passed() {
    set_listing(&[]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    assert!(slot.tick(t0).is_empty());
    set_listing(&["new"]);
    assert!(slot.tick(t0 + Duration::from_secs(29)).is_empty());
    assert_eq!(slot.tick(t0 + Duration::from_secs(30)).len(), 1);
}

#[test]
fn several_new_children_fire_in_name_order() {
    set_listing(&[]);
    let t0 = Instant::now();
    let mut slot = slot(t0);
    slot.tick(t0);
    set_listing(&["b", "a"]);
    let events = slot.tick(t0 + Duration::from_secs(30));
    let paths: Vec<String> = events
        .iter()
        .map(|e| e.payload["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        paths,
        [
            PathBuf::from("/work").join("a").to_string_lossy(),
            PathBuf::from("/work").join("b").to_string_lossy()
        ]
    );
}
```

4. Replace the whole of `src-tauri/crates/trigger-git/src/worker.rs` with exactly this.

```rust
//! The thread that joins the OS layer to the pure logic. It owns every
//! repository slot, waits for file events and for the polled kinds' times,
//! and sends events to the sink. Glue only: the decisions are in `logic`.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

mod folder;
mod repo;

// The allow is removed when lib.rs uses these.
#[allow(unused_imports)]
pub use folder::FolderSlot;
#[allow(unused_imports)]
pub use repo::{Entry, RepoSlot};
```

5. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git worker::folder` runs 4 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git worker::repo` still runs 15 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Watching the folder with `notify`. The folder is listed on the interval only.
- Scanning deeper than one level, or following the `.git` file of a worktree.
- Calling `FolderSlot` from `lib.rs` or the thread (T26, T27).
- README changes (T27).

## If stuck

- If `worker.rs` is not exactly the 12 lines T23 wrote (`wc -l` prints 12) before step 4, or `worker/repo.rs` has no `pub(crate) fn event`, T23 was built differently from its card. Set status blocked.
- If clippy reports `unused_imports` for `pub use folder::FolderSlot`, the `#[allow(unused_imports)]` line above it was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: blocked before step 4. The "If stuck" rule says worker.rs must be 13 to 15 lines before step 4. `wc -l src-tauri/crates/trigger-git/src/worker.rs` gives 12. The file holds only `//!` header lines, `#![allow(dead_code)]`, `mod repo;`, and `pub use repo::{Entry, RepoSlot};` with its allow. `worker/repo.rs` has `pub(crate) fn event` at line 65, so that check passed. Step 1 passed (`worker/folder.rs` does not exist). No files were changed except this card.
- 2026-10-12 architect: the 13 to 15 bound was a miscount; T23's worker.rs is 12 lines and matches its card. Fixed the If stuck line, reset to todo.
