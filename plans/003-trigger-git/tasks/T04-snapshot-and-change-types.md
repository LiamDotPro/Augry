---
id: T04
epic: 003-trigger-git
title: Add RepoSnapshot and Change to logic.rs
status: done
depends_on: [T03]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/logic/tests.rs
---

## Goal

`logic.rs` has `RepoSnapshot`, the `Change` enum (one variant per snapshot based kind), `Change::kind()` and `Change::branch()`, with unit tests. Nothing computes changes yet.

## Read first

- `plans/003-trigger-git/epic.md` Decisions 8 and 9
- `plans/003-trigger-git/research/R01-git-files.md` "Implications for the cards" (the snapshot fields)
- `src-tauri/crates/trigger-fs/src/logic.rs:1-40` the module doc and `#![allow(dead_code)]` style
- `src-tauri/crates/trigger-git/src/logic.rs` the placeholder you replace

## Steps

1. Replace the whole of `src-tauri/crates/trigger-git/src/logic.rs` with this. `RepoSnapshot` is Decision 8 exactly. `Change` has one variant for each of the eight kinds that come from comparing two snapshots; the three polled kinds (`behind-remote`, `uncommitted-older-than`, `repo-cloned`) have no variant here. The file ends with `mod tests;`, which loads the tests file from step 2. Tests live in their own file so `logic.rs` stays under the 400 line limit of `check-crates.mjs`.

```rust
//! Pure decisions: which changes a pair of repository snapshots shows. No git
//! calls, no threads, no clocks: everything comes in as arguments, so every
//! case is a unit test. The design is Decisions 8 and 9 of
//! plans/003-trigger-git/epic.md, checked against research note R01.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::collections::BTreeMap;

use trigger_core::Value;

pub type Payload = BTreeMap<String, Value>;

/// What the repository looked like at one moment, reduced to what the diff
/// needs. `refs` holds `refs/heads/*`, `refs/tags/*` and `refs/stash` only,
/// never remote refs, so a fetch changes nothing in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoSnapshot {
    /// Full sha of HEAD, None on an unborn branch.
    pub head: Option<String>,
    /// Current branch, None when HEAD is detached.
    pub branch: Option<String>,
    /// Sha in MERGE_HEAD, set only while a merge is in progress.
    pub merge_head: Option<String>,
    /// True while `rebase-merge/` or `rebase-apply/` exists.
    pub rebase: bool,
    /// Files with unresolved conflicts. Only read when `merge_head` is set.
    pub unmerged: u32,
    /// Subject of the newest HEAD reflog entry, such as `commit: fix`.
    pub reflog_subject: Option<String>,
    /// Short sha of HEAD.
    pub short: Option<String>,
    /// First line of the HEAD commit message.
    pub message: Option<String>,
    /// Ref name to sha.
    pub refs: BTreeMap<String, String>,
}

/// One thing that happened between two snapshots. Each variant is one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    RebaseStopped {
        branch: Option<String>,
        commit: Option<String>,
    },
    MergeConflict {
        branch: Option<String>,
        files: u32,
    },
    BranchSwitched {
        from: String,
        to: String,
        commit: Option<String>,
    },
    DetachedHead {
        from: String,
        commit: Option<String>,
    },
    Pulled {
        branch: Option<String>,
        commit: Option<String>,
        message: Option<String>,
    },
    Commit {
        branch: Option<String>,
        commit: Option<String>,
        message: Option<String>,
    },
    TagCreated {
        branch: Option<String>,
        tag: String,
    },
    StashCreated {
        branch: Option<String>,
    },
}

impl Change {
    /// The kind id this change fires.
    pub fn kind(&self) -> &'static str {
        match self {
            Change::RebaseStopped { .. } => "git.rebase-stopped",
            Change::MergeConflict { .. } => "git.merge-conflict",
            Change::BranchSwitched { .. } => "git.branch-switched",
            Change::DetachedHead { .. } => "git.detached-head",
            Change::Pulled { .. } => "git.pulled",
            Change::Commit { .. } => "git.commit",
            Change::TagCreated { .. } => "git.tag-created",
            Change::StashCreated { .. } => "git.stash-created",
        }
    }

    /// The branch a rule's `branch` glob is matched against: the new branch
    /// for a switch, the branch left for a detach, the current branch for the
    /// rest.
    pub fn branch(&self) -> Option<&str> {
        match self {
            Change::BranchSwitched { to, .. } => Some(to),
            Change::DetachedHead { from, .. } => Some(from),
            Change::RebaseStopped { branch, .. }
            | Change::MergeConflict { branch, .. }
            | Change::Pulled { branch, .. }
            | Change::Commit { branch, .. }
            | Change::TagCreated { branch, .. }
            | Change::StashCreated { branch } => branch.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests;
```

2. Create the folder `src-tauri/crates/trigger-git/src/logic/` and in it the file `tests.rs` with this content.

```rust
use super::*;

#[test]
fn kind_ids_are_kinds_of_this_crate() {
    let changes = [
        Change::RebaseStopped {
            branch: None,
            commit: None,
        },
        Change::MergeConflict {
            branch: None,
            files: 1,
        },
        Change::BranchSwitched {
            from: "a".into(),
            to: "b".into(),
            commit: None,
        },
        Change::DetachedHead {
            from: "a".into(),
            commit: None,
        },
        Change::Pulled {
            branch: None,
            commit: None,
            message: None,
        },
        Change::Commit {
            branch: None,
            commit: None,
            message: None,
        },
        Change::TagCreated {
            branch: None,
            tag: "v1".into(),
        },
        Change::StashCreated { branch: None },
    ];
    let mut seen = Vec::new();
    for change in &changes {
        assert!(
            crate::KINDS.iter().any(|k| k.id == change.kind()),
            "{} is not in KINDS",
            change.kind()
        );
        assert!(!seen.contains(&change.kind()), "{} twice", change.kind());
        seen.push(change.kind());
    }
}

#[test]
fn branch_is_the_branch_a_glob_matches_against() {
    let switched = Change::BranchSwitched {
        from: "old".into(),
        to: "new".into(),
        commit: None,
    };
    assert_eq!(switched.branch(), Some("new"));
    let detached = Change::DetachedHead {
        from: "main".into(),
        commit: None,
    };
    assert_eq!(detached.branch(), Some("main"));
    let commit = Change::Commit {
        branch: Some("dev".into()),
        commit: None,
        message: None,
    };
    assert_eq!(commit.branch(), Some("dev"));
    let tag = Change::TagCreated {
        branch: None,
        tag: "v1".into(),
    };
    assert_eq!(tag.branch(), None);
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`. It should change nothing.

Note on `check-crates.mjs`: `check-crates.mjs` prints warnings, not errors, about template text. The ones it may print for this crate until slice C are: `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code`. Warnings are fine here; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 10 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (see the note under Steps about the warnings it prints)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- The `payload` method (T05) and `step` (T06 to T08).
- Adding fields to `RepoSnapshot` or variants to `Change`, or renaming any.
- Types for the polled kinds, `Armed`, `Signal` or `State`. Slices B and C add what they need.
- Removing `#![allow(dead_code)]`.

## If stuck

- If clippy reports an unused import, the code differs from step 1. Re-copy it.
- If the folder `src/logic/` already exists with a `tests.rs`, set status blocked and say so.
- If `cargo fmt` rewrites the files heavily, you mis-pasted; re-copy.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
