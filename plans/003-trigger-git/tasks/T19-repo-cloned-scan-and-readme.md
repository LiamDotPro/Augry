---
id: T19
epic: 003-trigger-git
title: Add the repo-cloned scan logic and update the README
status: todo
depends_on: [T18]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-git/src/logic/polled.rs
  - src-tauri/crates/trigger-git/src/logic/polled_tests.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

`CloneScan` is the pure rule for `git.repo-cloned` (new children of a folder that hold a repository; the first scan is the baseline) with `cloned_payload`. The README describes the new files and the polled kinds' behaviour. This closes slice B.

## Read first

- `plans/003-trigger-git/epic.md` Decision 10 (`repo-cloned`)
- `src-tauri/crates/trigger-git/src/logic/polled.rs` the file you append to
- `src-tauri/crates/trigger-git/src/logic/polled_tests.rs` the tests you append to
- `src-tauri/crates/trigger-git/src/lib.rs` the `PAYLOAD_CLONED` list
- `src-tauri/crates/trigger-git/README.md` the sections `## Develop` and `## Known limits`

## Steps

1. In `src-tauri/crates/trigger-git/src/logic/polled.rs`, replace the line `use trigger_core::Value;` with these lines (blank line between the groups):

```rust
use std::collections::BTreeSet;
use std::path::Path;

use trigger_core::Value;
```

   Then append this to the end of the file (it starts with a blank line).

```rust

/// `git.repo-cloned`: fires for each new child of a folder that holds a
/// repository. The first scan is the baseline.
#[derive(Debug, Default)]
pub struct CloneScan {
    known: Option<BTreeSet<String>>,
}

impl CloneScan {
    pub fn new() -> CloneScan {
        CloneScan::default()
    }

    /// Feeds one scan. `repos` is the names of the children of the folder
    /// that contain `.git` (the OS layer lists them). Returns the names that
    /// were not in the previous scan, in sorted order.
    ///
    /// - The first scan returns nothing, whatever it holds.
    /// - A repository that disappears is forgotten, so cloning it again
    ///   fires again.
    pub fn observe(&mut self, repos: BTreeSet<String>) -> Vec<String> {
        let new = match &self.known {
            Some(known) => repos.difference(known).cloned().collect(),
            None => Vec::new(),
        };
        self.known = Some(repos);
        new
    }
}

/// The payload of `git.repo-cloned`: exactly the fields listed in `KINDS`.
pub fn cloned_payload(folder: &Path, name: &str) -> Payload {
    let mut out = Payload::new();
    let path = folder.join(name);
    out.insert("path".into(), Value::from(path.to_string_lossy().as_ref()));
    out
}
```

2. In `src-tauri/crates/trigger-git/src/logic/polled_tests.rs`, replace the first line `use super::polled::*;` with these lines:

```rust
use std::collections::BTreeSet;
use std::path::Path;

use super::polled::*;
```

   Then append this to the end of the file (it starts with a blank line).

```rust

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_first_scan_is_a_baseline() {
    let mut scan = CloneScan::new();
    assert!(scan.observe(names(&["a", "b"])).is_empty());
    assert!(scan.observe(names(&["a", "b"])).is_empty());
}

#[test]
fn a_new_child_fires_once_in_sorted_order() {
    let mut scan = CloneScan::new();
    scan.observe(names(&["a"]));
    assert_eq!(scan.observe(names(&["a", "z", "c"])), vec!["c", "z"]);
    assert!(scan.observe(names(&["a", "z", "c"])).is_empty());
}

#[test]
fn an_empty_first_scan_still_makes_the_second_one_count() {
    let mut scan = CloneScan::new();
    assert!(scan.observe(BTreeSet::new()).is_empty());
    assert_eq!(scan.observe(names(&["fresh"])), vec!["fresh"]);
}

#[test]
fn a_removed_and_cloned_again_repository_fires_again() {
    let mut scan = CloneScan::new();
    scan.observe(names(&["a", "b"]));
    assert!(scan.observe(names(&["a"])).is_empty());
    assert_eq!(scan.observe(names(&["a", "b"])), vec!["b"]);
}

#[test]
fn cloned_payload_has_exactly_the_listed_fields() {
    let folder = Path::new("/work/projects");
    let payload = cloned_payload(folder, "new-repo");
    assert_eq!(keys(&payload), listed("git.repo-cloned"));
    let expected = folder.join("new-repo");
    assert_eq!(payload["path"], expected.to_string_lossy().as_ref());
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-git`.

4. README. In `src-tauri/crates/trigger-git/README.md`, under `## Develop`, insert these two table rows directly above the row that starts with ``| `src/os.rs` |`` (below the rows T16 added):

```
| `src/logic/repo.rs` | The `Repo` trait: what the glue asks about a repository. `src/logic/fake.rs` is the fake used in tests |
| `src/logic/polled.rs` | Pure transitions for the polled kinds: `BehindWatch`, `UncommittedWatch`, `CloneScan`, and their payloads |
```

   Under `## Known limits`, directly below the last bullet (the one that starts ``- `git.uncommitted-older-than` uses the modified time``), add:

```
- The polled kinds check once per `interval_s`, not on file events, so an event can arrive up to one interval late.
- `git.behind-remote` fires when the count goes from 0 to above 0. The first check is only a baseline, and behind by 2 and then by 5 is one event. A repository with no upstream, or a detached HEAD, never fires it.
- `git.uncommitted-older-than` fires once and waits for a clean tree before it can fire again. Work that is already old when the rule starts fires on the first check.
- `git.repo-cloned` sees only direct children of `folder` that contain `.git`. The first scan is a baseline, so repositories already there fire nothing.
```

Note on `check-crates.mjs`: it prints warnings, not errors, about template text. Until slice C it may print `README.md still has template text`, `src/lib.rs still has template code`, `src/os.rs still has template code` and a length warning for `src/logic/tests.rs`. Warnings are fine; only errors fail the check. Do not try to remove them.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 93 tests in the library, 1 in `readme`, `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-git polled_tests` runs 17 tests and they pass
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `grep -c "CloneScan" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `grep -c "baseline" src-tauri/crates/trigger-git/README.md` prints `2`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Listing the folder's children. That is OS code and belongs in `os.rs` (slice C); `CloneScan::observe` takes the names it found.
- Scanning deeper than one level, or watching the folder with `notify`.
- Writing the Platforms, Permissions or Example sections (slice C).
- Changing the Kinds table, Rule fields or Payload sections.

## If stuck

- If the file `polled.rs` does not end with `uncommitted_payload`, T18 was built differently from its card. Set status blocked.
- If `cloned_payload_has_exactly_the_listed_fields` fails on the field names, `KINDS` lists different payload fields than T02 defined. Set status blocked and quote both.
- If the README has no `## Known limits` section, no ``| `src/os.rs` |`` row, or no bullet starting ``- `git.uncommitted-older-than` uses``, T16 was built differently from its card. Set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
