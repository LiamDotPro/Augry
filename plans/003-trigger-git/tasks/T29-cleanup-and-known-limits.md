---
id: T29
epic: 003-trigger-git
title: Remove temporary allows and finish the README
status: done
depends_on: [T28]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/src/git.rs
  - src-tauri/crates/trigger-git/src/git/repo.rs
  - src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/README.md
---

## Goal

The last temporary lines are gone: `#![allow(dead_code)]` in `git.rs` and `logic.rs`, the leftover `allow(unused_imports)`, and the unused `GitRepo::path`. The README Known limits are complete. `check-crates.mjs` shows no template warning for `trigger-git`. This closes slice C.

## Read first

- `src-tauri/crates/trigger-git/src/git.rs` the top of the file and the last lines (`pub use repo::GitRepo;`)
- `src-tauri/crates/trigger-git/src/logic.rs` the top of the file
- `src-tauri/crates/trigger-git/src/git/repo.rs:1-30` the imports and `GitRepo::path`
- `src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs:1-5` the imports
- `src-tauri/crates/trigger-git/README.md` the section `## Known limits`
- `plans/_tools/check-crates.mjs` what counts as template text

## Steps

1. In `src-tauri/crates/trigger-git/src/git.rs` delete these two lines and the blank line after them (near the top, below the `//!` comment):

```rust
// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]
```

   Near the end of the same file delete the line `// Removed when lib.rs uses GitRepo.` and the line `#[allow(unused_imports)]` that follows it. Keep `pub use repo::GitRepo;`.

2. In `src-tauri/crates/trigger-git/src/logic.rs` delete the same two lines and the blank line after them.

3. In `src-tauri/crates/trigger-git/src/git/repo.rs` delete this method and the blank line after it. Nothing calls it.

```rust
    pub fn path(&self) -> &Path {
        &self.path
    }
```

   Then change the import line `use std::path::{Path, PathBuf};` to `use std::path::PathBuf;`.

4. In `src-tauri/crates/trigger-git/src/git/repo/poll_tests.rs`, add the line `use std::path::Path;` as the first line of the file, above `use std::time::Duration;`.

5. Run `cd src-tauri && cargo fmt -p trigger-git`.

6. README. Append these bullets to the very end of `src-tauri/crates/trigger-git/README.md`, under `## Known limits`, directly after the last bullet.

```text
- A rebase that finishes within one 300 ms settle window is not seen as stopped, so `git.rebase-stopped` fires only for a rebase that stays stopped. The same merging means two commits made within 300 ms can give one `git.commit`.
- Submodules and bare repositories are untested. Repositories made with `--ref-format=reftable` are watched but untested.
- `start` fails for the whole source if one rule names a folder that is not a git repository, and it runs a few git calls per repository (the baseline snapshot) before it returns.
- A `git fetch` for `fetch: true` can take up to 30 seconds, and the thread does not notice a stop until it ends.
- A rule with a `branch` glob on `git.behind-remote` or `git.uncommitted-older-than` uses the branch of the latest snapshot. While the glob does not match, the rule is not checked and its memory is cleared, so the first check after a match is a baseline again.
- The hub's two second cool down per rule applies on top of the 300 ms settle.
```

7. Run `node plans/_tools/check-crates.mjs` and read its output. Lines about `trigger-core`, `trigger-hub` or `trigger-fs` are not yours. For `trigger-git` the only line allowed is `warn  trigger-git: src/logic/tests.rs has ... lines; consider splitting it`.

Note on `check-crates.mjs`: it prints warnings, not errors. A warning about `src/logic/tests.rs` being long is expected and stays. Only errors fail the check.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-git` passes
- [ ] `cd src-tauri && cargo test -p trigger-git -- --ignored --test-threads=1` passes (the library line `1 passed`, the `live` line `7 passed`)
- [ ] `grep -rn "allow(dead_code)\|allow(unused_imports)\|not implemented yet\|Template: remove" src-tauri/crates/trigger-git/src src-tauri/crates/trigger-git/README.md` prints nothing (exit code 1)
- [ ] `node plans/_tools/check-crates.mjs | grep "trigger-git"` prints at most the one line about `src/logic/tests.rs` being long; no line says `template`
- [ ] `grep -c "settle window" src-tauri/crates/trigger-git/README.md` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain | grep -v 'plans/003-trigger-git/tasks/'` lists only paths under `files:`

## Out of scope

- Splitting `src/logic/tests.rs` (it is over 400 lines, which is a warning and not an error) or any other refactor.
- Fixing a failing live test by changing `src/`. Set status blocked instead.
- Registering the source in the app, the repo map, CI or the feature flag (slice D).
- Editing the Kinds table, Rule fields, Payload, Platforms, Permissions or Example sections of the README.
- Adding `#[allow(...)]` anywhere to make clippy pass.

## If stuck

- If clippy reports `dead_code` for anything other than what this card removes, quote the item in the Log and set status blocked. Do not delete or allow it.
- If clippy still reports `GitRepo::path` as used somewhere, run `grep -rn "\.path()" src-tauri/crates/trigger-git/src` and quote the result; set status blocked.
- If the compiler says `Path` is not found in a file under `src/git/`, add `use std::path::Path;` to that file's imports (only in a test file) and note it in the Log. If it is in a non test file, set status blocked.
- If the `grep` in Acceptance prints a line, quote it in the Log and set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
