---
id: T13
epic: 003-trigger-git
title: Add GitRepo and read HEAD, branch, commit and refs
status: todo
depends_on: [T12]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git.rs
  - src-tauri/crates/trigger-git/src/git/repo.rs
  - src-tauri/crates/trigger-git/src/git/repo/tests.rs
---

## Goal

`GitRepo` exists in `src/git/repo.rs` (re-exported as `git::GitRepo`) and can read a `RepoSnapshot` with `head`, `branch`, `short`, `message`, `reflog_subject` and `refs` from a real repository. The merge, rebase and conflict fields come in T14.

## Read first

- `plans/003-trigger-git/epic.md` Decision 8 (what the snapshot holds)
- `plans/003-trigger-git/research/R01-git-files.md` "Cheap reads for the snapshot"
- `src-tauri/crates/trigger-git/src/logic.rs:15-45` `RepoSnapshot`, the type this fills
- `src-tauri/crates/trigger-git/src/git.rs` `run`, `value`, `TIMEOUT`
- `src-tauri/crates/trigger-git/src/git/testutil.rs` `TempRepo`, `git_available`, `scratch_path`

## Steps

1. Create `src-tauri/crates/trigger-git/src/git/repo.rs` with exactly this content. `GitRepo` lives in its own file so `git.rs` stays under the 400 line limit; `git.rs` re-exports it. The method `read_snapshot` is private on purpose: T16 adds the `Repo` impl that calls it.

```rust
//! `GitRepo`: a repository on disk, answered by running git in its work tree.
//! The pure parsing of git's output is in small functions at the bottom so it
//! has tests that need no git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{run, value, TIMEOUT};
use crate::logic::RepoSnapshot;

/// A repository on disk, read by running git in its work tree.
#[derive(Debug, Clone)]
pub struct GitRepo {
    path: PathBuf,
}

impl GitRepo {
    /// `path` is the work tree root, the rule's `repo`.
    pub fn new(path: impl Into<PathBuf>) -> GitRepo {
        GitRepo { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn value(&self, args: &[&str]) -> Option<String> {
        value(&self.path, args)
    }

    /// Reads the repository into a `RepoSnapshot`. Each field comes from one
    /// read only git call, and a call that fails leaves its field empty, so an
    /// unborn branch (no commits yet) is a snapshot with `head: None`.
    fn read_snapshot(&self) -> Result<RepoSnapshot, String> {
        let probe =
            run(&self.path, &["rev-parse", "--git-dir"], TIMEOUT).map_err(|e| e.to_string())?;
        if !probe.ok {
            return Err(format!("not a git repository: {}", self.path.display()));
        }
        let head = self.value(&["rev-parse", "--verify", "-q", "HEAD"]);
        let branch = self.value(&["symbolic-ref", "-q", "--short", "HEAD"]);
        let (short, message) = self
            .value(&["log", "-1", "--format=%h%x00%s", "HEAD"])
            .map(|text| parse_log(&text))
            .unwrap_or_default();
        let reflog_subject = self.value(&["reflog", "-1", "--format=%gs", "HEAD"]);
        // Local branches, tags and the stash only. Remote refs stay out so a
        // fetch changes nothing in the snapshot (Decision 8).
        let refs = self
            .value(&[
                "for-each-ref",
                "--count=2000",
                "--format=%(refname)%00%(objectname)",
                "refs/heads",
                "refs/tags",
                "refs/stash",
            ])
            .map(|text| parse_refs(&text))
            .unwrap_or_default();
        Ok(RepoSnapshot {
            head,
            branch,
            reflog_subject,
            short,
            message,
            refs,
            ..RepoSnapshot::default()
        })
    }
}

/// Splits the output of `log -1 --format=%h%x00%s` into the short sha and the
/// first line of the message. Either is None when empty.
pub(crate) fn parse_log(text: &str) -> (Option<String>, Option<String>) {
    let (short, message) = text.split_once('\0').unwrap_or((text, ""));
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (some(short), some(message))
}

/// Parses lines of `refname NUL sha` into a map. Lines without the NUL are
/// skipped.
pub(crate) fn parse_refs(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(name, sha)| (name.to_string(), sha.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
```

2. Create the folder `src-tauri/crates/trigger-git/src/git/repo/` and in it the file `tests.rs` with exactly this content.

```rust
use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

#[test]
fn parse_log_splits_short_sha_and_subject() {
    assert_eq!(
        parse_log("abc1234\0Fix the thing"),
        (Some("abc1234".into()), Some("Fix the thing".into()))
    );
    assert_eq!(parse_log("abc1234\0"), (Some("abc1234".into()), None));
    assert_eq!(parse_log(""), (None, None));
}

#[test]
fn parse_refs_reads_name_and_sha_per_line() {
    let text = "refs/heads/main\0aaa\nrefs/tags/v1\0bbb\nrefs/stash\0ccc\nnonsense\n";
    let refs = parse_refs(text);
    assert_eq!(refs.len(), 3);
    assert_eq!(refs["refs/heads/main"], "aaa");
    assert_eq!(refs["refs/tags/v1"], "bbb");
    assert_eq!(refs["refs/stash"], "ccc");
    assert!(parse_refs("").is_empty());
}

#[test]
fn a_repository_with_no_commits_has_a_branch_and_no_head() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.branch.as_deref(), Some("main"));
    assert_eq!(snap.head, None);
    assert_eq!(snap.short, None);
    assert_eq!(snap.message, None);
    assert_eq!(snap.reflog_subject, None);
    assert!(snap.refs.is_empty());
}

#[test]
fn a_commit_shows_in_head_message_reflog_and_refs() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first commit");
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    let head = snap.head.clone().unwrap();
    assert_eq!(head.len(), 40);
    assert_eq!(snap.branch.as_deref(), Some("main"));
    assert!(head.starts_with(snap.short.as_deref().unwrap()));
    assert_eq!(snap.message.as_deref(), Some("first commit"));
    assert!(snap.reflog_subject.unwrap().starts_with("commit"));
    assert_eq!(snap.refs["refs/heads/main"], head);
    assert_eq!(snap.refs.len(), 1);
}

#[test]
fn tags_the_stash_and_other_branches_are_in_refs() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.git(&["tag", "v1"]);
    temp.git(&["branch", "other"]);
    temp.write("a.txt", "changed\n");
    temp.git(&["stash", "push", "-q"]);
    let refs = GitRepo::new(&temp.path).read_snapshot().unwrap().refs;
    let names: Vec<&str> = refs.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "refs/heads/main",
            "refs/heads/other",
            "refs/stash",
            "refs/tags/v1"
        ]
    );
}

#[test]
fn a_detached_head_has_a_head_and_no_branch() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.git(&["checkout", "-q", "--detach"]);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.branch, None);
    assert!(snap.head.is_some());
}

#[test]
fn a_folder_that_is_not_a_repository_is_an_error() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("plain");
    std::fs::create_dir_all(&dir).unwrap();
    let err = GitRepo::new(&dir).read_snapshot().unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(err.starts_with("not a git repository"), "{err}");

    let missing = GitRepo::new(scratch_path("missing")).read_snapshot();
    assert!(missing.is_err());
}
```

3. In `src-tauri/crates/trigger-git/src/git.rs`, find the first line `#[cfg(test)]` (it is followed by a line starting `mod `). Directly above it, insert this block, leaving one blank line before the `#[cfg(test)]`.

```rust
mod repo;

// Removed when lib.rs uses GitRepo.
#[allow(unused_imports)]
pub use repo::GitRepo;
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 61 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git git::repo` runs 7 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- `merge_head`, `rebase` and `unmerged` (T14). They stay at their defaults here.
- `behind` and the oldest change (T15, T16), and `impl Repo for GitRepo` (T16).
- Remote refs in `refs`. The `for-each-ref` call lists `refs/heads`, `refs/tags` and `refs/stash` only, and that is deliberate (Decision 8).
- Changing the 2000 ref cap or the 3 second timeout.
- Calling `GitRepo` from `lib.rs`.

## If stuck

- If `cargo test` fails inside `a_commit_shows_in_head_message_reflog_and_refs` on `reflog_subject`, run `git reflog -1 --format=%gs HEAD` by hand in a repo with one commit and quote the output in the Log. Set status blocked.
- If clippy reports `unused_imports` for `pub use repo::GitRepo`, the `#[allow(unused_imports)]` line above it was lost. Put it back.
- If git is not installed, the git tests print `skipped: git is not installed` and pass. Add a Log line saying so.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
