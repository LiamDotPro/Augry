---
id: T07
epic: 003-trigger-git
title: Add branch switch, detached head, commit and pull to step
status: todo
depends_on: [T06]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/tests.rs
---

## Goal

`step` also reports `branch-switched`, `detached-head`, `pulled` and `commit`, with the reflog subject deciding between pulled, commit and nothing.

## Read first

- `plans/003-trigger-git/epic.md` Decision 9 (the exact choices for commit and pulled)
- `plans/003-trigger-git/research/R01-git-files.md` "Diff rules" (switched, detached, commit, pulled) and the file table
- `src-tauri/crates/trigger-git/src/logic.rs` the `step` function from T06

## Steps

1. In `src-tauri/crates/trigger-git/src/logic.rs`, find the line `    // T07 adds the branch switch, detached head, pull and commit here.` Replace that one line with this block (keep the blank lines around it as they are).

```rust
    // A switch needs a branch on both sides. A new branch made by
    // `checkout -b` is a switch too.
    if let (Some(from), Some(to)) = (&prev.branch, &next.branch) {
        if from != to {
            out.push(Change::BranchSwitched {
                from: from.clone(),
                to: to.clone(),
                commit: next.short.clone(),
            });
        }
    }

    // Leaving a branch for a bare commit. A merge in progress is not that.
    if let (Some(from), None) = (&prev.branch, &next.branch) {
        if next.merge_head.is_none() {
            out.push(Change::DetachedHead {
                from: from.clone(),
                commit: next.short.clone(),
            });
        }
    }

    // HEAD moved with the branch unchanged: the reflog subject says why. A
    // reset or the end of a rebase has another subject and fires nothing.
    if prev.branch == next.branch && next.head.is_some() && prev.head != next.head {
        let subject = next.reflog_subject.as_deref().unwrap_or("");
        if subject.starts_with("pull") || subject.starts_with("merge") {
            out.push(Change::Pulled {
                branch: next.branch.clone(),
                commit: next.short.clone(),
                message: next.message.clone(),
            });
        } else if subject.starts_with("commit") || subject.starts_with("cherry-pick") {
            out.push(Change::Commit {
                branch: next.branch.clone(),
                commit: next.short.clone(),
                message: next.message.clone(),
            });
        }
    }
```

How the pull and commit choice works: HEAD moved on the same branch (or both snapshots are detached). A reflog subject starting `pull` or `merge` is `Pulled`; one starting `commit` (which covers `commit (amend)`, `commit (merge)`, `commit (initial)`) or `cherry-pick` is `Commit`; anything else (a reset, the end of a rebase, no subject) fires nothing.

2. In `src-tauri/crates/trigger-git/src/logic/tests.rs`, append this to the end of the file, after a blank line.

```rust
/// `prev` on main at one commit, `next` on main at another with `subject` as
/// the newest reflog entry.
fn moved(subject: Option<&str>) -> (RepoSnapshot, RepoSnapshot) {
    let prev = on("main", "aaaaaaa1");
    let mut next = on("main", "bbbbbbb2");
    next.reflog_subject = subject.map(String::from);
    (prev, next)
}

#[test]
fn a_new_commit_fires_commit_with_its_message() {
    for subject in [
        "commit: add the thing",
        "commit (amend): add the thing",
        "commit (merge): Merge branch x",
        "commit (initial): first",
        "cherry-pick: add the thing",
    ] {
        let (prev, next) = moved(Some(subject));
        assert_eq!(
            step(&prev, &next),
            vec![Change::Commit {
                branch: Some("main".into()),
                commit: Some("bbbbbbb".into()),
                message: Some("message of bbbbbbb2".into())
            }],
            "{subject}"
        );
    }
}

#[test]
fn the_first_commit_on_an_unborn_branch_fires_commit() {
    let mut prev = on("main", "x");
    prev.head = None;
    prev.short = None;
    let (_, next) = moved(Some("commit (initial): first"));
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.commit"]);
}

#[test]
fn a_pull_or_merge_fires_pulled() {
    for subject in [
        "pull: Fast-forward",
        "pull: Merge made by the 'ort' strategy.",
        "pull --rebase (finish): returning to refs/heads/main",
        "merge origin/main: Fast-forward",
    ] {
        let (prev, next) = moved(Some(subject));
        assert_eq!(kinds(&step(&prev, &next)), vec!["git.pulled"], "{subject}");
    }
}

#[test]
fn a_reset_or_a_finished_rebase_fires_nothing() {
    for subject in [
        Some("reset: moving to HEAD~1"),
        Some("rebase (finish): returning to refs/heads/main"),
        Some("checkout: moving from a to main"),
        Some(""),
        None,
    ] {
        let (prev, next) = moved(subject);
        assert!(step(&prev, &next).is_empty(), "{subject:?}");
    }
}

#[test]
fn a_commit_on_a_detached_head_fires_commit() {
    let prev = detached("aaaaaaa1");
    let mut next = detached("bbbbbbb2");
    next.reflog_subject = Some("commit: wip".into());
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.commit"]);
}

#[test]
fn switching_branch_fires_only_the_switch() {
    let prev = on("main", "aaaaaaa1");
    // The new branch is at another commit and the reflog says checkout.
    let mut next = on("feature", "bbbbbbb2");
    next.reflog_subject = Some("checkout: moving from main to feature".into());
    assert_eq!(
        step(&prev, &next),
        vec![Change::BranchSwitched {
            from: "main".into(),
            to: "feature".into(),
            commit: Some("bbbbbbb".into())
        }]
    );
    // A subject starting with commit or pull cannot turn a switch into more.
    next.reflog_subject = Some("pull: Fast-forward".into());
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.branch-switched"]);
}

#[test]
fn a_new_branch_made_at_the_same_commit_is_a_switch() {
    let prev = on("main", "aaaaaaa1");
    let next = on("topic", "aaaaaaa1");
    assert_eq!(kinds(&step(&prev, &next)), vec!["git.branch-switched"]);
}

#[test]
fn leaving_a_branch_for_a_commit_fires_detached_head() {
    let prev = on("main", "aaaaaaa1");
    let next = detached("bbbbbbb2");
    assert_eq!(
        step(&prev, &next),
        vec![Change::DetachedHead {
            from: "main".into(),
            commit: Some("bbbbbbb".into())
        }]
    );
}

#[test]
fn getting_back_onto_a_branch_from_a_detached_head_fires_nothing() {
    let prev = detached("aaaaaaa1");
    let next = on("main", "bbbbbbb2");
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn a_detach_during_a_merge_is_not_a_detached_head() {
    let prev = on("main", "aaaaaaa1");
    let mut next = detached("aaaaaaa1");
    next.merge_head = Some("ccccccc3".into());
    assert!(step(&prev, &next).is_empty());
}

#[test]
fn a_conflict_and_a_switch_in_one_window_come_in_order() {
    let prev = on("main", "aaaaaaa1");
    let mut next = on("feature", "bbbbbbb2");
    next.merge_head = Some("ccccccc3".into());
    next.unmerged = 1;
    assert_eq!(
        kinds(&step(&prev, &next)),
        vec!["git.merge-conflict", "git.branch-switched"]
    );
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 29 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Tags and stashes (T08).
- Counting commits with `rev-list`, or any other tiebreak than the reflog subject. Decision 9 chose the subject.
- Reporting a switch when one side is detached.
- Changing the order of the blocks.

## If stuck

- If a test fails, compare the code with step 1 character by character; do not change the test.
- If the placeholder comment `// T07 adds ...` is missing, T06 did not land as written. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
