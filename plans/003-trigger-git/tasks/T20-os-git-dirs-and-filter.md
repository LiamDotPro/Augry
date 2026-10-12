---
id: T20
epic: 003-trigger-git
title: Add GitDirs to os.rs: git folders and event filter
status: done
depends_on: [T19]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/os.rs
  - src-tauri/crates/trigger-git/src/os/tests.rs
  - src-tauri/crates/trigger-git/src/git.rs
---

## Goal

`os.rs` can find the two git folders of a repository (`GitDirs::resolve`), list the folders to watch, say whether a changed path matters (`is_relevant`: `HEAD`, `MERGE_HEAD`, `packed-refs`, the rebase folders, local refs and reftable files count; `index`, `logs/`, `objects/`, `FETCH_HEAD`, lock files and remote refs do not), say whether git holds a lock, and report `availability`. Pure parts are unit tested; `resolve` is tested on real temporary repositories, including a linked worktree.

## Read first

- `plans/003-trigger-git/epic.md` Decision 6
- `plans/003-trigger-git/research/R01-git-files.md` "Implications for the cards" (the watch list, items 1 to 6, and the lock files)
- `src-tauri/crates/trigger-git/src/os.rs` the file you replace (10 lines)
- `src-tauri/crates/trigger-git/src/git.rs` `run`, `value`, `TIMEOUT`, and the last lines where `mod testutil;` is declared
- `src-tauri/crates/trigger-git/src/git/testutil.rs` `TempRepo` (`new`, `path`, `commit`, `git`), `git_available`, `scratch_path`

## Steps

1. Check the starting point: `ls src-tauri/crates/trigger-git/src` lists `git.rs`, `lib.rs`, `logic.rs`, `os.rs`, `rule.rs` and the folders `git` and `logic`, and there is no `os` folder and no `worker` file. If `src/os/` exists, use If stuck.

2. In `src-tauri/crates/trigger-git/src/git.rs`, at the end of the file, find the two lines

```rust
#[cfg(test)]
mod testutil;
```

   and change the second one to `pub(crate) mod testutil;`, so `os.rs` tests can use `TempRepo`. Change nothing else in `git.rs`.

3. Replace the whole of `src-tauri/crates/trigger-git/src/os.rs` with exactly this. The `#![allow(dead_code)]` stays until card T27. Decision 6 says to ignore a list of names; this file does the same thing the other way round, by naming the few files that count, which is why `ORIG_HEAD`, `CHERRY_PICK_HEAD` and `REBASE_HEAD` are left out too: none of them changes the snapshot.

```rust
//! The OS layer next to `git.rs`: where a repository keeps its files, which of
//! them matter, the `notify` watcher and the folder scan. The design is
//! Decision 6 of plans/003-trigger-git/epic.md and research note R01.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use notify::RecursiveMode;
use trigger_core::Availability;

use crate::git::{run, value, TIMEOUT};

/// Names directly inside a git dir that belong to one work tree and can change
/// what the snapshot shows. Everything else (`index`, `COMMIT_EDITMSG`,
/// `FETCH_HEAD`, `logs/`, `objects/`) is noise: `git status` alone rewrites
/// `index`. `packed-refs`, `refs/` and `reftable/` are shared and handled in
/// `relevant_inside`.
const OWN_NAMES: &[&str] = &["HEAD", "MERGE_HEAD", "rebase-merge", "rebase-apply"];

pub fn availability() -> Availability {
    match run(&std::env::temp_dir(), &["--version"], TIMEOUT) {
        Ok(out) if out.ok => Availability::Available,
        _ => Availability::Unsupported {
            reason: "git was not found".into(),
        },
    }
}

/// Where a repository keeps its files. In a normal repository the two paths
/// are the same `.git` folder. In a linked worktree `git_dir` is
/// `<main>/.git/worktrees/<name>` (HEAD, MERGE_HEAD, the rebase folders) and
/// `common_dir` is `<main>/.git` (refs, packed-refs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDirs {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

impl GitDirs {
    /// Asks git for the two folders of the repository whose work tree is
    /// `repo`. Both are made canonical, because macOS reports `/private/var`
    /// for `/var` and Windows adds `\\?\`, and events are compared with them.
    pub fn resolve(repo: &Path) -> Result<GitDirs, String> {
        let git_dir = value(repo, &["rev-parse", "--absolute-git-dir"])
            .ok_or_else(|| "not a git repository".to_string())?;
        let common = value(repo, &["rev-parse", "--git-common-dir"])
            .ok_or_else(|| "not a git repository".to_string())?;
        // Git prints the common dir relative to the folder it ran in, such as
        // `.git`, unless it is outside the work tree.
        let common = PathBuf::from(common);
        let common = if common.is_relative() {
            repo.join(common)
        } else {
            common
        };
        Ok(GitDirs {
            git_dir: canonical(PathBuf::from(git_dir)),
            common_dir: canonical(common),
        })
    }

    /// The folders to watch and how deep. Whole folders, never single files,
    /// because git replaces files by renaming over them and because
    /// `MERGE_HEAD` and the rebase folders may not exist yet.
    pub fn watch_list(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut list = vec![(self.git_dir.clone(), RecursiveMode::NonRecursive)];
        if self.common_dir != self.git_dir {
            list.push((self.common_dir.clone(), RecursiveMode::NonRecursive));
        }
        let refs = self.common_dir.join("refs");
        if refs.is_dir() {
            list.push((refs, RecursiveMode::Recursive));
        }
        // Repositories made with `--ref-format=reftable` keep refs here.
        let reftable = self.common_dir.join("reftable");
        if reftable.is_dir() {
            list.push((reftable, RecursiveMode::NonRecursive));
        }
        list
    }

    /// True when a change at `path` can change what the snapshot shows.
    /// Lock files never count: git writes `<name>.lock` and renames it onto
    /// `<name>`, and the rename is what is reported next. Remote refs never
    /// count either, because the snapshot leaves them out and a fetch must
    /// fire nothing.
    pub fn is_relevant(&self, path: &Path) -> bool {
        let locked = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".lock"));
        if locked {
            return false;
        }
        // In a linked worktree `HEAD` and the rebase folders count only in
        // its own git dir: the main repository's `HEAD` is not ours.
        let own = path
            .strip_prefix(&self.git_dir)
            .map(|rel| relevant_inside(rel, true))
            .unwrap_or(false);
        own || path
            .strip_prefix(&self.common_dir)
            .map(|rel| relevant_inside(rel, false))
            .unwrap_or(false)
    }

    /// True while git is in the middle of writing the index or HEAD. The
    /// settle step waits a little longer when this is true.
    pub fn lock_held(&self) -> bool {
        self.git_dir.join("index.lock").exists() || self.git_dir.join("HEAD.lock").exists()
    }
}

/// The decision for a path given relative to a git dir. `own` is true for the
/// work tree's own git dir, false for the common dir of a linked worktree.
fn relevant_inside(rel: &Path, own: bool) -> bool {
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""));
    match parts.next() {
        Some("refs") => parts.next() != Some("remotes"),
        Some("reftable") | Some("packed-refs") => true,
        Some(name) => own && OWN_NAMES.contains(&name),
        None => false,
    }
}

#[cfg(test)]
mod tests;
```

4. Create the folder `src-tauri/crates/trigger-git/src/os/` and in it the file `tests.rs` with exactly this.

```rust
use std::path::PathBuf;

use notify::RecursiveMode;

use super::*;
use crate::git::testutil::{git_available, scratch_path, TempRepo};

fn plain() -> GitDirs {
    GitDirs {
        git_dir: PathBuf::from("/r/.git"),
        common_dir: PathBuf::from("/r/.git"),
    }
}

fn linked() -> GitDirs {
    GitDirs {
        git_dir: PathBuf::from("/main/.git/worktrees/wt"),
        common_dir: PathBuf::from("/main/.git"),
    }
}

fn relevant(dirs: &GitDirs, path: &str) -> bool {
    dirs.is_relevant(&PathBuf::from(path))
}

#[test]
fn the_files_that_change_the_snapshot_are_relevant() {
    let d = plain();
    for path in [
        "/r/.git/HEAD",
        "/r/.git/MERGE_HEAD",
        "/r/.git/packed-refs",
        "/r/.git/rebase-merge",
        "/r/.git/rebase-merge/done",
        "/r/.git/rebase-apply",
        "/r/.git/refs/heads/main",
        "/r/.git/refs/heads/feature/a",
        "/r/.git/refs/tags/v1",
        "/r/.git/refs/stash",
        "/r/.git/reftable/tables.list",
    ] {
        assert!(relevant(&d, path), "{path}");
    }
}

#[test]
fn noise_is_not_relevant() {
    let d = plain();
    for path in [
        "/r/.git/index",
        "/r/.git/COMMIT_EDITMSG",
        "/r/.git/FETCH_HEAD",
        "/r/.git/ORIG_HEAD",
        "/r/.git/logs/HEAD",
        "/r/.git/logs/refs/heads/main",
        "/r/.git/objects/ab/cdef",
        "/r/.git/config",
        "/r/.git/refs/remotes/origin/main",
        "/r/.git",
        "/r/src/main.rs",
        "/elsewhere/HEAD",
    ] {
        assert!(!relevant(&d, path), "{path}");
    }
}

#[test]
fn lock_files_are_never_relevant() {
    let d = plain();
    for path in [
        "/r/.git/HEAD.lock",
        "/r/.git/index.lock",
        "/r/.git/refs/heads/main.lock",
        "/r/.git/refs/stash.lock",
        "/r/.git/packed-refs.lock",
        "/r/.git/reftable/tables.list.lock",
    ] {
        assert!(!relevant(&d, path), "{path}");
    }
}

#[test]
fn a_linked_worktree_uses_both_folders() {
    let d = linked();
    assert!(relevant(&d, "/main/.git/worktrees/wt/HEAD"));
    assert!(relevant(&d, "/main/.git/worktrees/wt/MERGE_HEAD"));
    assert!(relevant(&d, "/main/.git/worktrees/wt/rebase-merge/done"));
    assert!(relevant(&d, "/main/.git/refs/heads/wtb"));
    assert!(relevant(&d, "/main/.git/packed-refs"));
    assert!(!relevant(&d, "/main/.git/worktrees/wt/index"));
    assert!(!relevant(&d, "/main/.git/worktrees/wt/logs/HEAD"));
    // Another worktree's files are not ours.
    assert!(!relevant(&d, "/main/.git/worktrees/other/HEAD"));
    assert!(!relevant(&d, "/main/.git/HEAD"));
}

#[test]
fn the_watch_list_is_folders_with_refs_recursive() {
    let dir = scratch_path("dirs");
    std::fs::create_dir_all(dir.join("refs/heads")).unwrap();
    let d = GitDirs {
        git_dir: dir.clone(),
        common_dir: dir.clone(),
    };
    assert_eq!(
        d.watch_list(),
        vec![
            (dir.clone(), RecursiveMode::NonRecursive),
            (dir.join("refs"), RecursiveMode::Recursive),
        ]
    );
    std::fs::create_dir_all(dir.join("reftable")).unwrap();
    assert_eq!(
        d.watch_list().last(),
        Some(&(dir.join("reftable"), RecursiveMode::NonRecursive))
    );
    let other = GitDirs {
        git_dir: dir.join("worktrees/wt"),
        common_dir: dir.clone(),
    };
    let list = other.watch_list();
    assert_eq!(list[0].0, dir.join("worktrees/wt"));
    assert_eq!(list[1].0, dir);
    assert_eq!(list[2], (dir.join("refs"), RecursiveMode::Recursive));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lock_held_sees_index_and_head_locks_only() {
    let dir = scratch_path("lock");
    std::fs::create_dir_all(&dir).unwrap();
    let d = GitDirs {
        git_dir: dir.clone(),
        common_dir: dir.clone(),
    };
    assert!(!d.lock_held());
    std::fs::write(dir.join("ORIG_HEAD.lock"), "").unwrap();
    assert!(!d.lock_held());
    std::fs::write(dir.join("index.lock"), "").unwrap();
    assert!(d.lock_held());
    std::fs::remove_file(dir.join("index.lock")).unwrap();
    std::fs::write(dir.join("HEAD.lock"), "").unwrap();
    assert!(d.lock_held());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolve_finds_the_git_dir_of_a_normal_repository() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    let dirs = GitDirs::resolve(repo.path()).unwrap();
    let expected = std::fs::canonicalize(repo.path().join(".git")).unwrap();
    assert_eq!(dirs.git_dir, expected);
    assert_eq!(dirs.common_dir, expected);
}

#[test]
fn resolve_splits_the_two_folders_of_a_linked_worktree() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    repo.commit("a.txt", "first");
    let wt = scratch_path("wt");
    repo.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "wtb"]);
    let dirs = GitDirs::resolve(&wt).unwrap();
    let main_git = std::fs::canonicalize(repo.path().join(".git")).unwrap();
    assert_eq!(dirs.common_dir, main_git);
    assert!(dirs.git_dir.starts_with(main_git.join("worktrees")));
    assert_ne!(dirs.git_dir, dirs.common_dir);
    let _ = std::fs::remove_dir_all(&wt);
}

#[test]
fn resolve_fails_for_a_folder_that_is_not_a_repository() {
    if !git_available() {
        return;
    }
    let dir = scratch_path("plain");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        GitDirs::resolve(&dir),
        Err("not a git repository".to_string())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn git_is_available_on_a_machine_that_has_it() {
    if !git_available() {
        return;
    }
    assert_eq!(availability(), Availability::Available);
}
```

5. Run `cd src-tauri && cargo fmt -p trigger-git`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git os::tests` runs 10 tests and they pass
- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- The `notify` watcher, `scan_folder` and anything threaded (T21 and later).
- Removing `#![allow(dead_code)]` from `os.rs` (T27 does).
- Reading or parsing `packed-refs`, `HEAD` or any file in the git folder. Only the names of changed paths are looked at.
- Watching `index`, `logs/` or `objects/`.
- README changes (T27 writes the rows for `os.rs`).
- Making `testutil` public outside the crate, or touching any other file in `src/git/`.

## If stuck

- If `src/os/` already exists, or `os.rs` is longer than 20 lines, set status blocked and say so. Do not overwrite it.
- If step 2 finds no line `mod testutil;` in `git.rs`, T11 was built differently from its card. Set status blocked.
- If `resolve_splits_the_two_folders_of_a_linked_worktree` fails because `git worktree add` is unknown or errors, run `git --version` and quote it in the Log. Set status blocked.
- If a path test fails on Windows only, quote the path in the Log and set status blocked. Do not change the expected values.
- If clippy reports `dead_code` in `os.rs`, the line `#![allow(dead_code)]` was lost. Put it back.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
