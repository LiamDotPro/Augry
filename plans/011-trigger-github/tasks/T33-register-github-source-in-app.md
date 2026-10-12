---
id: T33
epic: 011-trigger-github
title: Register trigger-github in the app behind a feature
status: todo
depends_on: [T32]
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
---

## Goal

The app has a cargo feature `trigger-github` (on by default) that pulls in the `trigger-github` crate, and `sources()` registers `trigger_github::GithubSource` behind it, so the trigger catalogue lists the `github` source. A unit test proves it, and the app still builds with every source feature off.

## Read first

- `plans/011-trigger-github/epic.md` Decision 12
- `src-tauri/Cargo.toml:21-27` the `[features]` block and the first dependencies
- `src-tauri/src/triggers/mod.rs:29-41` `sources()`
- `src-tauri/src/triggers/mod.rs:352-360` the test `has_definition_looks_for_the_file`
- `src-tauri/crates/trigger-github/src/lib.rs` `GithubSource` is a unit struct (`pub struct GithubSource;`)

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It must not grow.

2. In `src-tauri/Cargo.toml` replace the `[features]` block with exactly:

```toml
[features]
default = ["trigger-fs", "trigger-github"]
trigger-fs = ["dep:trigger-fs"]
trigger-github = ["dep:trigger-github"]
```

3. In the same file, directly under the line `trigger-fs = { path = "crates/trigger-fs", optional = true }` in `[dependencies]` add:

```toml
trigger-github = { path = "crates/trigger-github", optional = true }
```

4. In `src-tauri/src/triggers/mod.rs`, in `sources()`, directly under the line `list.push(Box::new(trigger_fs::FsSource));` add:

```rust
    #[cfg(feature = "trigger-github")]
    list.push(Box::new(trigger_github::GithubSource));
```

Do not change anything else in the function.

5. In the same file, inside `mod tests`, add these two tests directly after the `has_definition_looks_for_the_file` test (the `Source` trait is already in scope through `use super::*`):

```rust
    #[cfg(feature = "trigger-github")]
    #[test]
    fn the_catalogue_lists_the_github_source() {
        assert!(sources().iter().any(|source| source.id() == "github"));
    }

    #[cfg(not(feature = "trigger-github"))]
    #[test]
    fn the_catalogue_has_no_github_source_without_the_feature() {
        assert!(!sources().iter().any(|source| source.id() == "github"));
    }
```

6. Run `cd src-tauri && cargo fmt` once, then the acceptance commands. `Cargo.lock` gains a `trigger-github` entry in the dependency list of `terminal-grid`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid the_catalogue_lists_the_github_source` passes (1 test)
- [ ] `cd src-tauri && cargo test --workspace` passes (the `trigger-github` live tests are `#[ignore]`d and do not run)
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo test -p terminal-grid --no-default-features the_catalogue_has_no_github_source_without_the_feature` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features --features trigger-github` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- Changing any file under `src-tauri/crates/`. The crate is finished.
- `.github/workflows/ci.yml`: T35.
- The repo map, architecture table and epic status: T34.
- Any change to `TriggerService`, the commands, Settings or the frontend. The catalogue already reaches the UI.
- Fixing the app's existing clippy warnings.

## If stuck

- If `cargo check` says `unresolved import trigger_github` or `use of undeclared crate`, the `[dependencies]` line from step 3 is missing or has no `optional = true`; fix it.
- If `cargo test --workspace` fails inside `trigger-github` because `gh` is missing or not signed in, set status blocked and name the test; its non-ignored tests must not need `gh`.
- If `Cargo.lock` is not updated because the build is offline (`--locked` or network errors), run `cd src-tauri && cargo metadata --offline > /dev/null`; the crate is a path dependency so it needs no download. If that fails too, set status blocked.
- If the clippy count grew, show the new warning lines in the Log; if they come from the new test code, fix them, otherwise set status blocked.
- If the `--no-default-features` test run fails to link or build for a reason unrelated to this card (for example tauri system libraries), run only the two `cargo check` commands and note the skipped test in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
