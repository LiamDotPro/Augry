---
id: T21
epic: 003-trigger-git
title: Add RepoWatch and scan_folder to os.rs
status: done
depends_on: [T20]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/os.rs
  - src-tauri/crates/trigger-git/src/os/tests.rs
---

## Goal

`os.rs` has `RepoWatch`, the `notify` watchers over a repository's git folders that call a function for every event that matters, `event_matters` (the pure filter on a `notify` event) and `scan_folder` (the children of a folder that hold a repository). One test with a real watcher, marked `#[ignore]`, proves that a commit, a branch switch and a tag are heard and that `git status` and `git add` are not.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 6 and 10 (`repo-cloned`)
- `plans/003-trigger-git/research/R01-git-files.md` "Doubts" (a rename onto `HEAD` must be seen on a non recursive watch)
- `src-tauri/crates/trigger-fs/src/os.rs:90-130` how `trigger-fs` starts a `notify` watcher
- `src-tauri/crates/trigger-git/src/os.rs` the file you edit
- `src-tauri/crates/trigger-git/src/os/tests.rs` the tests you append to

## Steps

1. In `src-tauri/crates/trigger-git/src/os.rs`, replace these lines near the top:

```rust
use std::path::{Path, PathBuf};

use notify::RecursiveMode;
use trigger_core::Availability;
```

   with these:

```rust
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecursiveMode, Watcher};
use trigger_core::Availability;
```

2. In the same file, find the last two lines, `#[cfg(test)]` and `mod tests;`. Insert this directly above them, leaving one blank line between the inserted code and `#[cfg(test)]`.

```rust
/// True when a `notify` event is about a file that can change the snapshot.
/// Pure, so the cases are unit tests.
pub fn event_matters(dirs: &GitDirs, event: &notify::Event) -> bool {
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event.paths.iter().any(|path| dirs.is_relevant(path))
}

/// Watchers over one repository. Dropping it stops them.
pub struct RepoWatch {
    _watcher: notify::RecommendedWatcher,
}

impl RepoWatch {
    /// Starts watching and returns at once. `on_change` runs on the watcher's
    /// own thread for every event that matters, so it must only send a
    /// message: no git calls in it. It may be called many times for one
    /// commit; the settle step in the caller merges them.
    pub fn start(
        dirs: &GitDirs,
        on_change: impl Fn() + Send + 'static,
    ) -> Result<RepoWatch, String> {
        let filter = dirs.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // A watcher error carries no path to act on; the next event puts
            // things right.
            let Ok(event) = res else { return };
            if event_matters(&filter, &event) {
                on_change();
            }
        })
        .map_err(|e| e.to_string())?;
        for (dir, mode) in dirs.watch_list() {
            watcher
                .watch(&dir, mode)
                .map_err(|e| format!("cannot watch {}: {e}", dir.display()))?;
        }
        Ok(RepoWatch { _watcher: watcher })
    }
}

/// The names of the children of `folder` that are repositories: folders that
/// contain `.git` (a folder, or a file in a linked worktree). Unreadable or
/// missing folders give an empty set.
pub fn scan_folder(folder: &Path) -> BTreeSet<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return BTreeSet::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir() && path.join(".git").exists()
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}
```

3. In `src-tauri/crates/trigger-git/src/os/tests.rs`, replace these lines at the top:

```rust
use std::path::PathBuf;

use notify::RecursiveMode;
```

   with these:

```rust
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use notify::event::{AccessKind, CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode};
```

   Then append this to the end of the file (it starts with a blank line).

```rust

fn event(kind: EventKind, paths: &[&str]) -> Event {
    let mut event = Event::new(kind);
    for path in paths {
        event = event.add_path(PathBuf::from(path));
    }
    event
}

#[test]
fn events_matter_by_path_and_kind() {
    let d = plain();
    let create = EventKind::Create(CreateKind::File);
    let data = EventKind::Modify(ModifyKind::Data(DataChange::Content));
    let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
    let remove = EventKind::Remove(RemoveKind::File);
    assert!(event_matters(&d, &event(create, &["/r/.git/refs/tags/v1"])));
    assert!(event_matters(&d, &event(data, &["/r/.git/HEAD"])));
    assert!(event_matters(&d, &event(remove, &["/r/.git/MERGE_HEAD"])));
    // A rename of the lock file onto HEAD names both paths.
    let onto = event(rename, &["/r/.git/HEAD.lock", "/r/.git/HEAD"]);
    assert!(event_matters(&d, &onto));
    assert!(!event_matters(&d, &event(data, &["/r/.git/index"])));
    assert!(!event_matters(&d, &event(create, &["/r/.git/HEAD.lock"])));
    let access = EventKind::Access(AccessKind::Any);
    assert!(!event_matters(&d, &event(access, &["/r/.git/HEAD"])));
    assert!(!event_matters(&d, &event(EventKind::Other, &["/r/.git/HEAD"])));
    assert!(!event_matters(&d, &event(data, &[])));
}

#[test]
fn a_rescan_flag_always_matters() {
    let flagged = Event::new(EventKind::Other).set_flag(Flag::Rescan);
    assert!(event_matters(&plain(), &flagged));
}

#[test]
fn scan_folder_lists_children_that_hold_a_repository() {
    let dir = scratch_path("scan");
    std::fs::create_dir_all(dir.join("app/.git")).unwrap();
    std::fs::create_dir_all(dir.join("plain")).unwrap();
    std::fs::create_dir_all(dir.join("outer/inner/.git")).unwrap();
    std::fs::create_dir_all(dir.join("linked")).unwrap();
    std::fs::write(dir.join("linked/.git"), "gitdir: /elsewhere").unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    let found: Vec<String> = scan_folder(&dir).into_iter().collect();
    assert_eq!(found, vec!["app".to_string(), "linked".to_string()]);
    assert!(scan_folder(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn expect_signal(rx: &Receiver<()>, what: &str) {
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("no signal for {what} within 10 s"));
}

/// Needs a real file system watcher. Run with:
/// `cargo test -p trigger-git os::tests::the_watcher -- --ignored --test-threads=1`
#[test]
#[ignore]
fn the_watcher_hears_a_commit_a_switch_and_a_tag_but_not_an_add() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let dirs = GitDirs::resolve(repo.path()).unwrap();
    let (tx, rx) = channel::<()>();
    let _watch = RepoWatch::start(&dirs, move || {
        let _ = tx.send(());
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));

    // `status` and `add` rewrite only the index, which is not watched.
    repo.git(&["status"]);
    repo.write("b.txt", "b");
    repo.git(&["add", "b.txt"]);
    assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());

    repo.git(&["commit", "-q", "-m", "second"]);
    expect_signal(&rx, "a commit");
    std::thread::sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}

    repo.git(&["checkout", "-q", "-b", "other"]);
    expect_signal(&rx, "a branch switch");
    std::thread::sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}

    repo.git(&["tag", "v1"]);
    expect_signal(&rx, "a tag");
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

5. Run the one test that needs a real watcher: `cd src-tauri && cargo test -p trigger-git os::tests::the_watcher -- --ignored --test-threads=1`. It takes about 3 seconds.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git os::tests` passes: 13 passed, 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git os::tests::the_watcher -- --ignored --test-threads=1` passes (1 passed)
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Using `RepoWatch` from `lib.rs` or a thread (T26 and T27).
- Calling git inside the watcher callback. It only calls `on_change`.
- The settle timer (T22) and the `Repo` trait.
- Watching `index`, `logs/` or `objects/`, or making the filter looser.
- Moving the real watcher test to `tests/live.rs`: an integration test cannot reach `GitDirs`. T28 covers the whole source there.
- README changes (T27).

## If stuck

- If step 1 or 3 cannot find the old import lines, T20 was built differently from its card. Set status blocked.
- If `the_watcher_hears_a_commit_a_switch_and_a_tag_but_not_an_add` fails with `no signal for ... within 10 s`, run it once more alone. If it fails again set status blocked and quote the output. Do not change the test or the filter.
- If it fails on `assert!(rx.recv_timeout(Duration::from_secs(1)).is_err())`, the watcher is reporting `index` changes. Set status blocked and quote the output.
- If `notify` does not compile (`RecommendedWatcher` or `Watcher` not found), the dependency is `notify = { workspace = true }` at version 8.2. Do not change versions; set status blocked.
- If clippy reports `dead_code`, `#![allow(dead_code)]` at the top of `os.rs` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
