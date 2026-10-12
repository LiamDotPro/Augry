---
id: T16
epic: 003-trigger-git
title: Add oldest change age and impl Repo for GitRepo
status: todo
depends_on: [T15]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git/repo.rs
  - src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

`GitRepo` reads the age of its oldest modified tracked file and implements `Repo`, so the real repository and the fake answer the same three questions. The README describes `git.rs`, `GitRepo` and the limits found so far.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`uncommitted-older-than`) and Decision 12
- `plans/003-trigger-git/research/R02-git-cli-pattern.md` "Rules for the crate" (porcelain v2 record shapes)
- `src-tauri/crates/trigger-git/src/git/repo.rs` the file you edit
- `src-tauri/crates/trigger-git/src/logic/repo.rs` the trait you implement
- `src-tauri/crates/trigger-git/README.md` the sections `## Develop` and `## Known limits`

## Steps

1. In `src-tauri/crates/trigger-git/src/git/repo.rs`, change the top imports. After the line `use std::path::{Path, PathBuf};` add the line `use std::time::SystemTime;`. Replace the line `use crate::logic::RepoSnapshot;` with these two lines, in this order:

```rust
use crate::logic::repo::Repo;
use crate::logic::RepoSnapshot;
```

2. Find the doc comment line ``    /// The sha in MERGE_HEAD (None unless a merge is in progress) and whether`` inside `impl GitRepo`. Directly above it insert this method, with one blank line between.

```rust
    /// Age in seconds of the oldest modified tracked file, from the modified
    /// times of the files `status` lists. Untracked files are left out, and so
    /// is a deleted file, which has no modified time to read.
    fn read_oldest_change_secs(&self) -> Option<u64> {
        let text = self.value(&["status", "--porcelain=v2", "-z", "--untracked-files=no"])?;
        let now = SystemTime::now();
        changed_paths(&text)
            .iter()
            .filter_map(|path| {
                let modified = std::fs::metadata(self.path.join(path)).ok()?.modified().ok()?;
                Some(age_secs(now, modified))
            })
            .max()
    }
```

3. Find the doc comment line ``/// Counts the `u` records of `status --porcelain=v2 -z`.`` (outside the `impl`). Directly above it insert this block, with one blank line between.

```rust
impl Repo for GitRepo {
    fn snapshot(&self) -> Result<RepoSnapshot, String> {
        self.read_snapshot()
    }

    fn behind(&self, fetch: bool) -> Option<u32> {
        self.read_behind(fetch)
    }

    fn oldest_change_secs(&self) -> Option<u64> {
        self.read_oldest_change_secs()
    }
}
```

4. Find the doc comment line ``/// Splits the output of `log -1 --format=%h%x00%s` into the short sha and the``. Directly above it insert these two functions, with one blank line between.

```rust
/// The paths in `status --porcelain=v2 -z --untracked-files=no`. Each record
/// starts with its type and then has a fixed number of space separated fields
/// before the path: `1` ordinary (8 before the path), `2` renamed or copied
/// (9, and the old path follows as the next record, which is skipped), `u`
/// unmerged (10). The path itself may contain spaces.
pub(crate) fn changed_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut records = text.split('\0');
    while let Some(record) = records.next() {
        let parts = match record.get(..2) {
            Some("1 ") => 9,
            Some("2 ") => {
                records.next();
                10
            }
            Some("u ") => 11,
            _ => continue,
        };
        if let Some(path) = record.splitn(parts, ' ').nth(parts - 1) {
            paths.push(path.to_string());
        }
    }
    paths
}

/// Whole seconds from `then` to `now`; 0 when `then` is in the future.
pub(crate) fn age_secs(now: SystemTime, then: SystemTime) -> u64 {
    now.duration_since(then).map(|d| d.as_secs()).unwrap_or(0)
}
```

5. In `src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs`, replace the first line `use super::*;` with these three lines (a blank line between the first two):

```rust
use std::time::Duration;

use super::*;
```

   Then append this to the end of the file.

```rust
#[test]
fn changed_paths_reads_every_record_type() {
    let text = concat!(
        "1 .M N... 100644 100644 100644 aaa bbb a.txt\0",
        "1 M. N... 100644 100644 100644 aaa bbb dir/with space.txt\0",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new name.txt\0old name.txt\0",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc conflict.txt\0",
    );
    assert_eq!(
        changed_paths(text),
        vec![
            "a.txt",
            "dir/with space.txt",
            "new name.txt",
            "conflict.txt"
        ]
    );
    assert!(changed_paths("").is_empty());
}

#[test]
fn age_secs_counts_back_and_never_goes_negative() {
    let now = SystemTime::now();
    assert_eq!(age_secs(now, now - Duration::from_secs(90)), 90);
    assert_eq!(age_secs(now, now + Duration::from_secs(90)), 0);
}

/// Sets a file's modified time to `secs` seconds ago.
fn age_file(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn a_clean_tree_or_only_untracked_files_has_no_oldest_change() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    let repo = GitRepo::new(&temp.path);
    assert_eq!(repo.read_oldest_change_secs(), None);
    temp.write("new.txt", "untracked\n");
    assert_eq!(repo.read_oldest_change_secs(), None);
}

#[test]
fn the_oldest_modified_tracked_file_sets_the_age() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    temp.commit("b.txt", "second");
    temp.write("a.txt", "changed a, a longer line\n");
    temp.write("b.txt", "changed b, an even longer line than a\n");
    age_file(&temp.path.join("a.txt"), 7200);
    age_file(&temp.path.join("b.txt"), 600);
    let age = GitRepo::new(&temp.path).read_oldest_change_secs().unwrap();
    assert!((7200..7260).contains(&age), "{age}");
}

#[test]
fn a_deleted_tracked_file_is_not_counted() {
    if !git_available() {
        return;
    }
    let temp = TempRepo::new();
    temp.commit("a.txt", "first");
    std::fs::remove_file(temp.path.join("a.txt")).unwrap();
    assert_eq!(GitRepo::new(&temp.path).read_oldest_change_secs(), None);
}

#[test]
fn a_git_repo_answers_the_repo_trait() {
    if !git_available() {
        return;
    }
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    clone.write("a.txt", "edited, longer than before\n");
    age_file(&clone.path.join("a.txt"), 3600);
    let repo: Box<dyn Repo> = Box::new(GitRepo::new(&clone.path));
    assert_eq!(repo.snapshot().unwrap().branch.as_deref(), Some("main"));
    assert_eq!(repo.behind(false), Some(0));
    assert!(repo.oldest_change_secs().unwrap() >= 3600);
}
```

6. Run `cd src-tauri && cargo fmt -p trigger-git`.

7. README. In `src-tauri/crates/trigger-git/README.md`, under `## Develop`, insert these two table rows directly above the row that starts with ``| `src/os.rs` |``:

```
| `src/git.rs` | The git command line, std only: `run` (timeout, no console window on Windows, a fallback to the Homebrew paths on macOS) and `value`. A copy of the app's pattern, since a trigger crate cannot depend on the app |
| `src/git/repo.rs` | `GitRepo`, the real `Repo`: builds a `RepoSnapshot` from read only git commands, counts commits behind the upstream, finds the age of the oldest uncommitted change |
```

   Under `## Known limits`, directly below the paragraph that starts `What does not work`, add a blank line and then this list:

```
- git must be installed. On macOS, if `git` is not on the PATH, `/opt/homebrew/bin/git` and then `/usr/local/bin/git` are tried.
- `repo` must be the root of the work tree, which is what `${repo.root}` gives.
- Every git call has a 3 second limit (30 seconds for `git fetch`). A call that times out leaves its part of the snapshot empty.
- Remote refs are not read, so a `git fetch` fires nothing. With `fetch: true` the fetch runs with `--no-tags`, so tags on the remote do not become local tags and do not fire `git.tag-created`. A fetch that fails (offline) is ignored and the count uses the remote refs already on disk.
- `git.uncommitted-older-than` uses the modified time of each tracked file that `git status` lists. Untracked files and deleted files are not counted, and a file saved again looks younger.
```

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 76 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git poll_tests` runs 10 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `grep -c "src/git.rs" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `grep -c "no-tags" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Using `GitRepo` from `lib.rs` or `os.rs`.
- Counting untracked files, or reading the time a change was first made (no file can tell that).
- Writing the Platforms, Permissions or Example sections, or the polled kinds' limits (T19 and slice C).
- Changing the Kinds table, Rule fields or Payload sections.

## If stuck

- If an anchor line in step 2, 3 or 4 is not found in `repo.rs`, T14 or T15 was built differently from its card. Set status blocked and say which line is missing.
- If `the_oldest_modified_tracked_file_sets_the_age` fails on the range, quote the age it printed in the Log and set status blocked. Do not widen the range.
- If `set_modified` is reported as unstable or missing, quote `rustc --version` in the Log and set status blocked.
- If the README has no `## Known limits` section or no ``| `src/os.rs` |`` row, set status blocked.
- If git is not installed, the git tests print `skipped: git is not installed` and pass. Add a Log line saying so.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
