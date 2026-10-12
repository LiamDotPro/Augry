---
id: T15
epic: 003-trigger-git
title: Count commits behind the upstream in GitRepo
status: todo
depends_on: [T14]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git/repo.rs
  - src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs
---

## Goal

`GitRepo::read_behind(fetch)` returns how many commits `HEAD` is behind its upstream, running `git fetch` first only when asked, tolerating a failed fetch, and never creating local tags. Tests use a real origin and clone.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`behind-remote`) and Decision 12 (`behind`)
- `plans/003-trigger-git/research/R02-git-cli-pattern.md` "Rules for the crate" (read only commands, fetch with a longer timeout)
- `src-tauri/crates/trigger-git/src/git/repo.rs` the file you edit
- `src-tauri/crates/trigger-git/src/git/testutil.rs` `TempRepo::clone_repo`

## Steps

1. In `src-tauri/crates/trigger-git/src/git/repo.rs`, change the line `use super::{run, value, TIMEOUT};` to `use super::{run, value, FETCH_TIMEOUT, TIMEOUT};`.

2. Find the doc comment line ``    /// The sha in MERGE_HEAD (None unless a merge is in progress) and whether`` inside `impl GitRepo`. Directly above it insert this method, leaving one blank line between them. `--no-tags` is deliberate: without it a fetch can create local tags, and the snapshot would report them as `git.tag-created`.

```rust
    /// Commits `HEAD` is behind its upstream, counted from the remote refs on
    /// disk. With `fetch` the remote is asked first; a fetch that fails
    /// (offline, no remote) is ignored and the count uses what is on disk.
    /// `--no-tags` keeps the fetch from creating local tags, which would show
    /// up as `git.tag-created`. None without an upstream or on a detached HEAD.
    fn read_behind(&self, fetch: bool) -> Option<u32> {
        if fetch {
            let _ = run(
                &self.path,
                &["fetch", "--quiet", "--no-tags"],
                FETCH_TIMEOUT,
            );
        }
        self.value(&["rev-list", "--count", "HEAD..@{u}"])?
            .parse()
            .ok()
    }
```

3. At the very end of `repo.rs`, after a blank line, add:

```rust
#[cfg(test)]
mod poll_tests;
```

4. Create `src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs` with exactly this content.

```rust
use super::*;
use crate::git::testutil::{git_available, TempRepo};

/// An origin with one commit and a clone of it.
fn pair() -> (TempRepo, TempRepo) {
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    (origin, clone)
}

#[test]
fn behind_counts_new_remote_commits_only_after_a_fetch() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    assert_eq!(repo.read_behind(false), Some(0));
    origin.commit("a.txt", "second");
    origin.commit("a.txt", "third");
    assert_eq!(repo.read_behind(false), Some(0));
    assert_eq!(repo.read_behind(true), Some(2));
    assert_eq!(repo.read_behind(false), Some(2));
    clone.git(&["merge", "--ff-only", "-q", "@{u}"]);
    assert_eq!(repo.read_behind(false), Some(0));
}

#[test]
fn behind_is_none_without_an_upstream_or_on_a_detached_head() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    assert_eq!(GitRepo::new(&origin.path).read_behind(false), None);
    clone.git(&["checkout", "-q", "--detach"]);
    assert_eq!(GitRepo::new(&clone.path).read_behind(false), None);
}

#[test]
fn a_fetch_that_fails_still_counts_from_the_refs_on_disk() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    origin.commit("a.txt", "second");
    assert_eq!(repo.read_behind(true), Some(1));
    drop(origin);
    assert_eq!(repo.read_behind(true), Some(1));
}

#[test]
fn a_fetch_changes_nothing_in_the_snapshot() {
    if !git_available() {
        return;
    }
    let (origin, clone) = pair();
    let repo = GitRepo::new(&clone.path);
    let before = repo.read_snapshot().unwrap();
    origin.commit("a.txt", "second");
    origin.git(&["tag", "v1"]);
    origin.git(&["branch", "side"]);
    assert_eq!(repo.read_behind(true), Some(1));
    assert_eq!(repo.read_snapshot().unwrap(), before);
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 70 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git poll_tests` runs 4 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- `impl Repo for GitRepo` (T16).
- The interval timer and the 0 to above 0 rule (T17). This method only answers one count.
- Fetching a named remote, prune, or credentials. A repo whose remote needs a login simply fails the fetch and uses the refs on disk (`GIT_TERMINAL_PROMPT=0` stops any prompt).
- Changing `FETCH_TIMEOUT` in `git.rs`.

## If stuck

- If step 2's anchor line is not found in `repo.rs`, T14 was built differently from its card. Set status blocked and say what the file has instead.
- If `behind_counts_new_remote_commits_only_after_a_fetch` fails with a count of 0 after the fetch, run `git fetch --quiet --no-tags` by hand in a clone and quote what it prints in the Log. Set status blocked.
- If git is not installed, the git tests print `skipped: git is not installed` and pass. Add a Log line saying so.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
