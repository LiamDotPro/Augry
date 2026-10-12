---
id: T06
epic: 003-trigger-git
title: Add step and observe: rebase stop and merge conflict
status: done
depends_on: [T05]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/tests.rs
---

## Goal

The pure `step(prev, next)` exists and reports `rebase-stopped` and `merge-conflict`, suppresses everything else while a rebase runs, and `observe` makes the first snapshot a silent baseline.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 8 and 9
- `plans/003-trigger-git/research/R01-git-files.md` "Diff rules" (merge-conflict and rebase-stopped lines)
- `src-tauri/crates/trigger-git/src/logic.rs` the file you add to

## Steps

`step` is Decision 9 plus R01's rules. The order in which changes are reported is: rebase stopped, merge conflict, branch switched, detached head, pulled or commit, tag created, stash created. This card writes the first two and leaves two marked gaps.

1. In `src-tauri/crates/trigger-git/src/logic.rs`, insert this block directly above the two lines `#[cfg(test)]` and `mod tests;` at the end of the file (one blank line between). The two `// T07 ...` and `// T08 ...` comment lines are placeholders that the next cards replace; leave them exactly as written.

```rust
/// The changes between two snapshots, in the order they are reported:
/// rebase stopped, merge conflict, branch switched, detached head, pulled or
/// commit, tag created, stash created. Each kind at most once.
pub fn step(prev: &RepoSnapshot, next: &RepoSnapshot) -> Vec<Change> {
    let mut out = Vec::new();

    // HEAD is detached and moves on every step of a rebase, so while one is
    // running only its start can be reported.
    if next.rebase {
        if !prev.rebase {
            out.push(Change::RebaseStopped {
                branch: prev.branch.clone(),
                commit: next.short.clone(),
            });
        }
        return out;
    }

    // A merge with no conflicting files (`merge --no-commit`) fires nothing.
    if prev.merge_head.is_none() && next.merge_head.is_some() && next.unmerged > 0 {
        out.push(Change::MergeConflict {
            branch: next.branch.clone(),
            files: next.unmerged,
        });
    }

    // T07 adds the branch switch, detached head, pull and commit here.

    // T08 adds the new tag and the new stash here.

    out
}

/// Like `step`, for the glue that may not have a previous snapshot yet. The
/// first snapshot is a baseline: it never fires anything.
pub fn observe(prev: Option<&RepoSnapshot>, next: &RepoSnapshot) -> Vec<Change> {
    match prev {
        Some(prev) => step(prev, next),
        None => Vec::new(),
    }
}
```

2. In `src-tauri/crates/trigger-git/src/logic/tests.rs`, append this to the end of the file, after a blank line. The helpers `on`, `detached` and `kinds` are used by the tests in the next cards too.

```rust
/// A snapshot on `branch` at `head`, with a short sha made from the head.
fn on(branch: &str, head: &str) -> RepoSnapshot {
    RepoSnapshot {
        head: Some(head.into()),
        branch: Some(branch.into()),
        short: Some(head.chars().take(7).collect()),
        message: Some(format!("message of {head}")),
        ..RepoSnapshot::default()
    }
}

/// A snapshot with HEAD detached at `head`.
fn detached(head: &str) -> RepoSnapshot {
    RepoSnapshot {
        branch: None,
        ..on("x", head)
    }
}

fn kinds(changes: &[Change]) -> Vec<&'static str> {
    changes.iter().map(Change::kind).collect()
}

#[test]
fn the_first_snapshot_fires_nothing() {
    let mut first = on("main", "aaaaaaa1");
    first.merge_head = Some("bbbbbbb2".into());
    first.unmerged = 2;
    first.rebase = true;
    assert!(observe(None, &first).is_empty());
    assert_eq!(observe(Some(&on("main", "a")), &first).len(), 1);
}

#[test]
fn an_unchanged_repository_fires_nothing() {
    let snap = on("main", "aaaaaaa1");
    assert!(step(&snap, &snap.clone()).is_empty());
}

#[test]
fn a_merge_conflict_reports_the_unmerged_count() {
    let prev = on("main", "aaaaaaa1");
    let mut next = prev.clone();
    next.merge_head = Some("bbbbbbb2".into());
    next.unmerged = 3;
    assert_eq!(
        step(&prev, &next),
        vec![Change::MergeConflict {
            branch: Some("main".into()),
            files: 3
        }]
    );
}

#[test]
fn a_merge_without_conflicts_or_one_already_running_fires_nothing() {
    let prev = on("main", "aaaaaaa1");
    let mut no_conflict = prev.clone();
    no_conflict.merge_head = Some("bbbbbbb2".into());
    no_conflict.unmerged = 0;
    assert!(step(&prev, &no_conflict).is_empty());

    let mut running = prev.clone();
    running.merge_head = Some("bbbbbbb2".into());
    running.unmerged = 3;
    let mut still = running.clone();
    still.unmerged = 2;
    assert!(step(&running, &still).is_empty());
}

#[test]
fn a_rebase_stop_fires_once_and_names_the_branch_it_left() {
    let prev = on("feature", "aaaaaaa1");
    let mut next = detached("ccccccc3");
    next.rebase = true;
    assert_eq!(
        step(&prev, &next),
        vec![Change::RebaseStopped {
            branch: Some("feature".into()),
            commit: Some("ccccccc".into())
        }]
    );
}

#[test]
fn nothing_else_fires_while_a_rebase_runs() {
    let prev = on("feature", "aaaaaaa1");
    let mut stopped = detached("ccccccc3");
    stopped.rebase = true;
    // Still rebasing, HEAD moved, a tag appeared, a conflict file is listed.
    let mut later = detached("ddddddd4");
    later.rebase = true;
    later.merge_head = Some("eeeeeee5".into());
    later.unmerged = 1;
    later.refs.insert("refs/tags/v1".into(), "f".into());
    assert!(step(&stopped, &later).is_empty());
    // Even a first sight of a rebase with a tag only reports the stop.
    assert_eq!(kinds(&step(&prev, &later)), vec!["git.rebase-stopped"]);
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 18 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Branch switch, detached head, pulled, commit, tag and stash (T07, T08).
- Changing the early `return` while a rebase runs: nothing but `RebaseStopped` may fire then, tags and stashes included.
- Any call to git, any clock, any thread.
- Using `observe` from `lib.rs`. The glue is slice C.

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- If clippy reports `dead_code` on `observe`, the `#![allow(dead_code)]` line was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
