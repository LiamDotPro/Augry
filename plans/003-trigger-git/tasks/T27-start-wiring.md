---
id: T27
epic: 003-trigger-git
title: Wire GitSource::start to the watchers and the thread
status: done
depends_on: [T26]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/lib.rs
  - src-tauri/crates/trigger-git/src/tests.rs
  - src-tauri/crates/trigger-git/src/os.rs
  - src-tauri/crates/trigger-git/src/rule.rs
  - src-tauri/crates/trigger-git/src/worker.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

`GitSource::start` works: it parses the rules, groups them by repository, takes each baseline snapshot, starts one file watcher per repository and the `trigger-git` thread, and returns a `Handle` whose drop stops the thread. The temporary `#![allow(dead_code)]` lines in `os.rs`, `rule.rs` and `worker.rs` are gone. The README says how it works on each platform and that it asks for no permission.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 4, 6, 10 and 13
- `src-tauri/crates/trigger-fs/src/lib.rs:128-165` `FsSource::start`, which this follows
- `src-tauri/crates/trigger-git/src/lib.rs:200-240` the end of the file: `GitSource` and its stub `start`
- `src-tauri/crates/trigger-git/src/worker.rs` `Msg`, `spawn`, and the exports `Entry`, `RepoSlot`, `FolderSlot`
- `src-tauri/crates/trigger-git/src/os.rs` `GitDirs::resolve`, `RepoWatch::start`, `scan_folder`
- `src-tauri/crates/trigger-git/README.md` the sections `## Platforms`, `## Permissions`, `## Develop`

## Steps

1. Check that `src-tauri/crates/trigger-git/src/tests.rs` does not exist.

2. In `src-tauri/crates/trigger-git/src/lib.rs` make these 2 edits, each a find and replace of exact text. Each find text appears once. Edit 1 adds the imports of `std` above the existing `use trigger_core::{...}` block, with a blank line between them.

**Edit 1.** Find:

```rust
use trigger_core::{
    Availability,
```

Replace with:

```rust
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use trigger_core::{
    Availability,
```

**Edit 2.** Find:

```rust
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        for r in &rules {
            rule::parse(r).map_err(|e| SourceError::Os(e.to_string()))?;
        }
        // The watcher and the thread are added by the glue card (slice C).
        let _ = sink;
        Err(SourceError::Unavailable("not implemented yet".into()))
    }
```

Replace with:

```rust
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        start_rules(rules, sink)
    }
```

3. Append this to the end of `src-tauri/crates/trigger-git/src/lib.rs` (it starts with a blank line). Everything that can fail happens here, before the thread starts, so the hub hears about it from `start`. A rule with a bad field, or a `repo` that is not a git repository, fails the whole call and names the rule or the folder.

```rust

/// Groups the rules by repository, takes a baseline snapshot of each, starts
/// one file watcher per repository and then the `trigger-git` thread that
/// owns them. Everything that can fail runs here, before the thread, so the
/// hub hears about it from `start`.
fn start_rules(rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
    let now = Instant::now();
    let mut by_repo: BTreeMap<PathBuf, Vec<worker::Entry>> = BTreeMap::new();
    let mut folders = Vec::new();
    for r in &rules {
        let parsed = rule::parse(r).map_err(|e| SourceError::Os(format!("{}: {e}", r.id)))?;
        if let Some(folder) = parsed.when.folder.clone() {
            let interval_s = parsed.when.interval_s;
            let list = os::scan_folder;
            folders.push(worker::FolderSlot::new(
                r.id.clone(),
                folder,
                interval_s,
                now,
                list,
            ));
        } else if let Some(repo) = parsed.when.repo.clone() {
            let entry = worker::Entry::new(r.id.clone(), r.kind.clone(), parsed, now);
            by_repo.entry(repo).or_default().push(entry);
        }
    }
    if by_repo.is_empty() && folders.is_empty() {
        return Ok(Handle::new(|| {}));
    }

    let (tx, rx) = mpsc::channel::<worker::Msg>();
    let mut slots = Vec::new();
    let mut keep: Vec<Box<dyn Send>> = Vec::new();
    for (index, (repo, entries)) in by_repo.into_iter().enumerate() {
        let dirs = os::GitDirs::resolve(&repo)
            .map_err(|e| SourceError::Os(format!("{}: {e}", repo.display())))?;
        let ping = tx.clone();
        let watch = os::RepoWatch::start(&dirs, move || {
            let _ = ping.send(worker::Msg::Changed(index));
        })
        .map_err(|e| SourceError::Os(format!("{}: {e}", repo.display())))?;
        keep.push(Box::new(watch));
        let locked = dirs.clone();
        slots.push(worker::RepoSlot::new(
            repo.to_string_lossy().into_owned(),
            Box::new(git::GitRepo::new(&repo)),
            Box::new(move || locked.lock_held()),
            entries,
        ));
    }

    worker::spawn(slots, folders, keep, rx, sink).map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(Handle::new(move || {
        let _ = tx.send(worker::Msg::Stop);
    }))
}

#[cfg(test)]
mod tests;
```

4. Create `src-tauri/crates/trigger-git/src/tests.rs` with exactly this content.

```rust
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::json;
use trigger_core::testkit::rule;
use trigger_core::{RuleError, Source, SourceError};

use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

fn commit_rule(repo: &std::path::Path) -> Rule {
    rule("p/t", "git.commit", json!({ "repo": repo }))
}

#[test]
fn the_source_describes_itself() {
    assert_eq!(GitSource.id(), "git");
    assert_eq!(GitSource.kinds().len(), 11);
    if git_available() {
        assert_eq!(GitSource.availability(), Availability::Available);
    }
}

#[test]
fn check_names_the_bad_field() {
    let bad = rule("p/t", "git.commit", json!({ "repo": "relative/dir" }));
    match GitSource.check(&bad) {
        Err(RuleError::Field { field, .. }) => assert_eq!(field, "repo"),
        other => panic!("expected a repo error, got {other:?}"),
    }
}

#[test]
fn start_fails_for_a_bad_rule_and_names_it() {
    let (tx, _rx) = channel();
    let bad = rule("p/bad", "git.commit", json!({}));
    match GitSource.start(vec![bad], tx) {
        Err(SourceError::Os(why)) => assert!(why.starts_with("p/bad: "), "{why}"),
        other => panic!("expected an error, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn start_fails_for_a_folder_that_is_not_a_repository() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("not-a-repo");
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, _rx) = channel();
    let result = GitSource.start(vec![commit_rule(&dir)], tx);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Err(SourceError::Os(why)) => assert!(why.contains("not a git repository"), "{why}"),
        other => panic!("expected an error, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn start_fails_for_a_folder_that_does_not_exist() {
    let (tx, _rx) = channel();
    let result = GitSource.start(vec![commit_rule(&scratch_path("missing"))], tx);
    assert!(matches!(result, Err(SourceError::Os(_))));
}

#[test]
fn start_with_no_rules_does_nothing_and_stops_cleanly() {
    let (tx, rx) = channel();
    let handle = GitSource.start(Vec::new(), tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn dropping_the_handle_stops_the_thread_within_a_second() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let (tx, rx) = channel();
    let other = rule("p/o", "git.tag-created", json!({ "repo": repo.path() }));
    let handle = GitSource
        .start(vec![commit_rule(repo.path()), other], tx)
        .unwrap();
    let started = Instant::now();
    drop(handle);
    // The thread owns the only sender, so the channel closes when it ends.
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(2)),
        Err(RecvTimeoutError::Disconnected)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_repo_cloned_rule_starts_and_stops_without_a_repository() {
    let folder = scratch_path("projects");
    std::fs::create_dir_all(&folder).unwrap();
    let (tx, rx) = channel();
    let cloned = rule("p/c", "git.repo-cloned", json!({ "folder": folder }));
    let handle = GitSource.start(vec![cloned], tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(2)),
        Err(RecvTimeoutError::Disconnected)
    ));
    let _ = std::fs::remove_dir_all(&folder);
}
```

5. Remove the temporary allows. In each of `src/os.rs`, `src/rule.rs` and `src/worker.rs`, delete these two lines and the blank line after them (they are near the top of the file, below the `//!` comment):

```rust
// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]
```

   In `src/worker.rs` also delete the line `// The allow is removed when lib.rs uses these.` and both lines `#[allow(unused_imports)]` (there are two, one above `pub use folder::FolderSlot;` and one above `pub use repo::{Entry, RepoSlot};`). Leave the `pub use` lines. Do not touch `git.rs` or `logic.rs` (T29).

6. README. In `src-tauri/crates/trigger-git/README.md`:

   a. In the `## Platforms` table, replace the three rows `| macOS | | |`, `| Windows | | |` and `| Linux | | |` with these three rows.

```text
| macOS | FSEvents through `notify`, on the repository's git folders only. Then the `git` command line | git comes from the PATH, then `/opt/homebrew/bin/git`, then `/usr/local/bin/git`. The live tests pass here |
| Windows | `ReadDirectoryChangesW` through `notify`. Then the `git` command line, run with no console window | git must be on the PATH (Git for Windows puts it there). The live tests have not been run on Windows yet |
| Linux | inotify through `notify`. Then the `git` command line | Each watched repository uses a few inotify watches (the git folder and every folder under `refs/`). The live tests have not been run on Linux yet |
```

   b. Under `## Permissions`, replace the line that starts `What the OS asks the user for` with this paragraph.

```text
None. The crate reads the repository folders named in the rules and runs the `git` command line; it asks the OS for no permission. macOS may ask the user once before an app reads a repository inside Documents, Desktop or Downloads. That prompt belongs to the OS, not to this crate.
```

   c. In the `## Develop` table, replace the whole row that starts ``| `src/lib.rs` |`` with:

```text
| `src/lib.rs` | `KINDS`, `GitSource` and `start_rules`: parses the rules, groups them by repository, takes each baseline snapshot, starts the file watchers and the `trigger-git` thread. Glue only |
```

   d. In the same table, replace the whole row that starts ``| `src/logic.rs` |`` with:

```text
| `src/logic.rs` | Pure: `RepoSnapshot`, `Change`, `step` and `observe` (which changes two snapshots show) and the event payloads. Unit tested in `src/logic/tests.rs` |
```

   e. In the same table, replace the whole row that starts ``| `src/os.rs` |`` with the row below, and directly above it insert the two rows after it (so the order is `src/logic/polled.rs`, `src/logic/settle.rs`, `src/worker.rs`, `src/os.rs`).

```text
| `src/os.rs` | The only file besides `git.rs` that talks to the OS: `GitDirs` (where a repository keeps its files and which of them matter), `RepoWatch` (the `notify` watchers), `scan_folder` and `availability()` |
```

```text
| `src/logic/settle.rs` | Pure: `Settle`, the 300 ms quiet time before a snapshot, with up to 10 retries while `index.lock` or `HEAD.lock` exists |
| `src/worker.rs` | The `trigger-git` thread: `run` sleeps until the next deadline or message, `spawn` starts it. `src/worker/repo.rs` holds `RepoSlot` (one repository: snapshot, diff, rule filter, polled kinds) and `src/worker/folder.rs` holds `FolderSlot` (one `git.repo-cloned` folder). Both are tested with a fake `Repo` and no threads |
```

7. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git --lib start_` runs 4 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git --lib dropping_the_handle` runs 1 test and it passes
- [ ] `cd src-tauri && cargo test -p trigger-git` passes (this includes `tests/readme.rs`)
- [ ] `grep -c "allow(dead_code)" src-tauri/crates/trigger-git/src/os.rs src-tauri/crates/trigger-git/src/rule.rs src-tauri/crates/trigger-git/src/worker.rs` prints `0` for each file
- [ ] `grep -c "Glue only" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `grep -c "FSEvents" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Removing the allows in `git.rs` and `logic.rs`, or deleting `GitRepo::path` (T29).
- The live tests (T28) and the Known limits bullets (T29).
- Changing how rules are grouped: one slot per distinct `repo` path as written in the rule, one `FolderSlot` per `git.repo-cloned` rule.
- Skipping a rule whose repository cannot be found. The whole `start` fails; that is a documented limit.
- Registering the source in the app (slice D) or editing anything outside the crate.
- Touching the Kinds, Rule fields, Payload or Example sections of the README.

## If stuck

- If an edit's find text is not in `lib.rs`, T02 was built differently from its card. Set status blocked and say which edit.
- If the compiler cannot find `git::GitRepo`, check that `src/git.rs` has the line `pub use repo::GitRepo;`. If it does not, set status blocked.
- If `dropping_the_handle_stops_the_thread_within_a_second` fails, do not change the time limits: set status blocked and quote the output.
- If clippy reports `dead_code` after step 5 for something in `os.rs`, `rule.rs` or `worker.rs`, quote the item in the Log and set status blocked. Do not add an allow back.
- If the README has none of the rows or lines named in step 6, slice A or B was built differently from its cards. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
