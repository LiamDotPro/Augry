---
id: T14
epic: 003-trigger-git
title: Read merge, rebase and conflict state in GitRepo
status: done
depends_on: [T13]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git/repo.rs
  - src-tauri/crates/trigger-git/src/git/repo/tests.rs
---

## Goal

`GitRepo::read_snapshot` also fills `merge_head`, `rebase` and `unmerged`, for a plain repository and for a linked worktree, with tests on real merges and rebases.

## Read first

- `plans/003-trigger-git/epic.md` Decision 8 (`merge_head`, `rebase`, `unmerged`)
- `plans/003-trigger-git/research/R01-git-files.md` "Linked worktree" and "Cheap reads for the snapshot"
- `src-tauri/crates/trigger-git/src/git/repo.rs` the file you edit
- `src-tauri/crates/trigger-git/src/git/repo/tests.rs` the tests you append to

## Steps

All edits are in `src-tauri/crates/trigger-git/src/git/repo.rs` except step 5.

1. Find this block, the end of `read_snapshot`, and replace it with the block in step 1b. `unmerged` is only read while a merge is in progress, because counting it costs a `git status` (Decision 8).

```rust
        Ok(RepoSnapshot {
            head,
            branch,
            reflog_subject,
            short,
            message,
            refs,
            ..RepoSnapshot::default()
        })
```

1b. Replacement:

```rust
        let (merge_head, rebase) = self.operation_state();
        // Counting conflicts costs a `status`, so only a merge in progress
        // pays for it.
        let unmerged = if merge_head.is_some() {
            self.unmerged()
        } else {
            0
        };
        Ok(RepoSnapshot {
            head,
            branch,
            merge_head,
            rebase,
            unmerged,
            reflog_subject,
            short,
            message,
            refs,
        })
```

2. Find the doc comment line ``    /// Reads the repository into a `RepoSnapshot`. Each field comes from one`` (inside `impl GitRepo`). Directly above it, insert these two methods, leaving one blank line between them and the doc comment. `--git-path` finds the files in a plain repository and in a linked worktree alike, so no path is joined by hand.

```rust
    /// The sha in MERGE_HEAD (None unless a merge is in progress) and whether
    /// a rebase is in progress. `--git-path` finds the files for a plain
    /// repository and for a linked worktree alike, so no path is built by hand.
    fn operation_state(&self) -> (Option<String>, bool) {
        let paths = self.value(&[
            "rev-parse",
            "--git-path",
            "MERGE_HEAD",
            "--git-path",
            "rebase-merge",
            "--git-path",
            "rebase-apply",
        ]);
        let Some(paths) = paths else {
            return (None, false);
        };
        // The paths are relative to the work tree, or absolute; `join` copes
        // with both.
        let mut lines = paths.lines().map(|line| self.path.join(line));
        let merge_head = lines
            .next()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.trim().to_string())
            .filter(|sha| !sha.is_empty());
        let rebase = lines.any(|dir| dir.exists());
        (merge_head, rebase)
    }

    /// Files with unresolved conflicts, from `status`.
    fn unmerged(&self) -> u32 {
        self.value(&["status", "--porcelain=v2", "-z", "--untracked-files=no"])
            .map(|text| count_unmerged(&text))
            .unwrap_or(0)
    }
```

3. Find the doc comment line ``/// Splits the output of `log -1 --format=%h%x00%s` into the short sha and the`` (outside the `impl`, near the end of the file). Directly above it, insert this function.

```rust
/// Counts the `u` records of `status --porcelain=v2 -z`. A rename record (`2`)
/// is followed by one more record holding the old path, which is skipped so a
/// path that happens to start with `u ` is not counted.
pub(crate) fn count_unmerged(text: &str) -> u32 {
    let mut records = text.split('\0');
    let mut count = 0;
    while let Some(record) = records.next() {
        if record.starts_with("2 ") {
            records.next();
        } else if record.starts_with("u ") {
            count += 1;
        }
    }
    count
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-git`.

5. In `src-tauri/crates/trigger-git/src/git/repo/tests.rs`, append this to the end of the file. The file already imports `scratch_path` and, through `use super::*;`, `run` and `TIMEOUT`.

```rust
#[test]
fn count_unmerged_counts_u_records_and_skips_rename_sources() {
    let text = concat!(
        "1 .M N... 100644 100644 100644 aaa bbb a.txt\0",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc b.txt\0",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new.txt\0u old name\0",
        "u AA N... 000000 100644 100644 100644 aaa bbb ccc c.txt\0",
    );
    assert_eq!(count_unmerged(text), 2);
    assert_eq!(count_unmerged(""), 0);
}

/// Commits on `main` and on `other` that both change `a.txt`, left on `main`.
fn diverged(temp: &TempRepo) {
    temp.commit("a.txt", "base");
    temp.git(&["checkout", "-q", "-b", "other"]);
    temp.commit("a.txt", "change on other");
    temp.git(&["checkout", "-q", "main"]);
    temp.commit("a.txt", "change on main");
}

#[test]
fn a_quiet_repository_has_no_merge_or_rebase() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert_eq!(snap.merge_head, None);
    assert!(!snap.rebase);
    assert_eq!(snap.unmerged, 0);
}

#[test]
fn a_conflicted_merge_shows_merge_head_and_the_file_count() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    assert!(!temp.try_git(&["merge", "other"]).ok);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    let other = temp.git(&["rev-parse", "other"]);
    assert_eq!(snap.merge_head, Some(other));
    assert_eq!(snap.unmerged, 1);
    assert!(!snap.rebase);
    assert_eq!(snap.branch.as_deref(), Some("main"));
}

#[test]
fn a_rebase_that_stops_shows_rebase_and_a_detached_head() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    assert!(!temp.try_git(&["rebase", "other"]).ok);
    let snap = GitRepo::new(&temp.path).read_snapshot().unwrap();
    assert!(snap.rebase);
    assert_eq!(snap.branch, None);
    assert!(snap.head.is_some());
    assert_eq!(snap.merge_head, None);
    assert_eq!(snap.unmerged, 0);
}

#[test]
fn a_linked_worktree_finds_its_own_merge_state() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    diverged(&temp);
    let linked = scratch_path("linked");
    temp.git(&["worktree", "add", "-q", linked.to_str().unwrap(), "other"]);
    let wt = GitRepo::new(&linked);
    assert_eq!(wt.read_snapshot().unwrap().merge_head, None);
    let merge = run(&linked, &["merge", "main"], TIMEOUT).unwrap();
    assert!(!merge.ok);
    let snap = wt.read_snapshot().unwrap();
    assert!(snap.merge_head.is_some());
    assert_eq!(snap.unmerged, 1);
    // The main work tree is untouched.
    assert_eq!(
        GitRepo::new(&temp.path).read_snapshot().unwrap().merge_head,
        None
    );
    std::fs::remove_dir_all(&linked).unwrap();
}
```

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 66 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git git::repo` runs 12 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Counting conflicts when no merge is in progress. Decision 8 says `unmerged` is read only when `merge_head` is set; a stopped rebase therefore has `unmerged: 0`.
- Reading `REBASE_HEAD`, `CHERRY_PICK_HEAD` or `ORIG_HEAD`.
- `behind` and the oldest change (T15, T16).
- Changing `RepoSnapshot` or `step`.

## If stuck

- If the text in step 1 or step 2 is not found in `repo.rs`, T13 was built differently from its card. Set status blocked and say which text is missing.
- If `a_rebase_that_stops_shows_rebase_and_a_detached_head` fails because the rebase did not stop, quote `git --version` and the test output in the Log and set status blocked.
- If clippy says `needless_update` on `..RepoSnapshot::default()`, step 1 left that line in. Remove it; all nine fields are named now.
- If git is not installed, the git tests print `skipped: git is not installed` and pass. Add a Log line saying so.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (66 lib tests, readme 1, live 1 ignored; git::repo 12 passed; clippy, fmt --check, check-crates 0 errors). Git was installed, no skips. Scratch files I accidentally wrote outside the worktree were removed.
- 2026-10-12 checker: pass
