---
id: T30
epic: 003-trigger-git
title: Register trigger-git in the app behind a feature
status: done
depends_on: [T29]
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
---

## Goal

The app has a cargo feature `trigger-git` (on by default) that pulls in the `trigger-git` crate, and `sources()` registers `trigger_git::GitSource` behind it, so the trigger catalogue lists the `git` source. A unit test proves it, and the app still builds with every source feature off.

## Read first

- `plans/003-trigger-git/epic.md` Decision 13
- `src-tauri/Cargo.toml:26-33` the `[features]` block and the first dependencies
- `src-tauri/src/triggers/mod.rs:31-41` `sources()`
- `src-tauri/src/triggers/mod.rs:246-251` the start of `mod tests` (its `use` lines)
- `src-tauri/src/triggers/mod.rs:354-359` the `has_definition_looks_for_the_file` test
- `src-tauri/crates/trigger-git/src/lib.rs:206` `GitSource` is a unit struct

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It must not grow.

2. In `src-tauri/Cargo.toml` replace the `[features]` block with exactly:

```toml
[features]
default = ["trigger-fs", "trigger-git"]
trigger-fs = ["dep:trigger-fs"]
trigger-git = ["dep:trigger-git"]
```

If the block already lists other `trigger-...` features (from another epic), keep them, add `"trigger-git"` to the end of `default`, and add the `trigger-git = ["dep:trigger-git"]` line after the last feature line.

3. In the same file, directly under the line `trigger-fs = { path = "crates/trigger-fs", optional = true }` in `[dependencies]` add (after any other optional `trigger-...` lines that are already there):

```toml
trigger-git = { path = "crates/trigger-git", optional = true }
```

4. In `src-tauri/src/triggers/mod.rs`, in `sources()`, directly under the line `list.push(Box::new(trigger_fs::FsSource));` add (after any other registered sources already there):

```rust
    #[cfg(feature = "trigger-git")]
    list.push(Box::new(trigger_git::GitSource));
```

Do not change anything else in the function.

5. In the same file, inside `mod tests`, add these tests directly after the `has_definition_looks_for_the_file` test. The `Source` trait is already in scope through `use super::*`. If a function named `source_ids` already exists in `mod tests`, do not add a second one; reuse it.

```rust
    fn source_ids() -> Vec<&'static str> {
        sources().iter().map(|source| source.id()).collect()
    }

    #[cfg(feature = "trigger-git")]
    #[test]
    fn the_catalogue_lists_the_git_source() {
        assert!(source_ids().contains(&"git"));
    }

    #[cfg(not(feature = "trigger-git"))]
    #[test]
    fn the_catalogue_has_no_git_source_without_the_feature() {
        assert!(!source_ids().contains(&"git"));
    }
```

Do not add an allow for `source_ids`; the second test uses it when the feature is off.

6. Run `cd src-tauri && cargo fmt` once, then the acceptance commands. `Cargo.lock` gains a `trigger-git` entry under `terminal-grid`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid the_catalogue_lists_the_git_source` passes (1 test)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo test -p terminal-grid --no-default-features the_catalogue_has_no_git_source_without_the_feature` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features --features trigger-git` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log (record the after number too)
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/003-trigger-git/tasks/`

## Out of scope

- Changing any file under `src-tauri/crates/`. The crate is finished.
- `.github/workflows/ci.yml`: T32.
- The repo map and architecture table: T31.
- Any change to `TriggerService`, the commands, Settings or the frontend. The catalogue already reaches the UI.
- Fixing the app's existing clippy warnings.

## If stuck

- If `cargo check` says `unresolved import trigger_git` or `use of undeclared crate`, the `[dependencies]` line from step 3 is missing or has no `optional = true`; fix it.
- If `cargo test --workspace` fails inside `trigger-git` only because `git` is not installed, set status blocked and name the test; only the ignored live tests may need git.
- If `Cargo.lock` is not updated because the build is offline (`--locked` or network errors), run `cd src-tauri && cargo metadata --offline > /dev/null`; the crate is a path dependency so it needs no download. If that fails too, set status blocked.
- If the clippy count grew, show the new warning lines in the Log; if they come from the new test code, fix them, otherwise set status blocked.
- If the `--no-default-features` test run fails to link or build for a reason unrelated to this card (for example tauri system libraries), run only the two `cargo check` commands and note the skipped test in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Clippy warnings (grep -c warning): baseline 7, after 7. Feature-on and feature-off targeted tests both pass; `cargo test --workspace` passed; no-default-features check, trigger-git check, fmt --check, trigger-git clippy -D warnings and check-crates all exit 0. Cargo.lock gained `"trigger-git"` under terminal-grid as expected.
- 2026-10-12 checker: pass
