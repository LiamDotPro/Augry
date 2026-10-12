---
id: T30
epic: 010-trigger-docker
title: Register trigger-docker in the app behind a feature
status: todo
depends_on: [T27]
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
---

## Goal

The app has a cargo feature `trigger-docker` (on by default) that pulls in the `trigger-docker` crate, and `sources()` registers `trigger_docker::DockerSource` behind it, so the trigger catalogue lists the `docker` source. A unit test proves it, and the app still builds with every source feature off.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 11
- `src-tauri/Cargo.toml:21-30` the `[features]` block and the first dependencies
- `src-tauri/src/triggers/mod.rs:29-41` `sources()`
- `src-tauri/src/triggers/mod.rs:218-235` the start of `mod tests` (its `use` lines)
- `src-tauri/crates/trigger-docker/src/lib.rs:131` `DockerSource` is a unit struct

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log (expected about 7). It must not grow.

2. In `src-tauri/Cargo.toml` replace the `[features]` block with exactly:

```toml
[features]
default = ["trigger-fs", "trigger-docker"]
trigger-fs = ["dep:trigger-fs"]
trigger-docker = ["dep:trigger-docker"]
```

3. In the same file, directly under the line `trigger-fs = { path = "crates/trigger-fs", optional = true }` in `[dependencies]` add:

```toml
trigger-docker = { path = "crates/trigger-docker", optional = true }
```

4. In `src-tauri/src/triggers/mod.rs`, in `sources()`, directly under the line `list.push(Box::new(trigger_fs::FsSource));` add:

```rust
    #[cfg(feature = "trigger-docker")]
    list.push(Box::new(trigger_docker::DockerSource));
```

Do not change anything else in the function.

5. In the same file, inside `mod tests`, add these two tests directly after the `has_definition_looks_for_the_file` test. The `Source` trait is already imported at the top of the file (`use trigger_core::{..., Source}` is reached through `use super::*`).

```rust
    fn source_ids() -> Vec<&'static str> {
        sources().iter().map(|source| source.id()).collect()
    }

    #[cfg(feature = "trigger-docker")]
    #[test]
    fn the_catalogue_lists_the_docker_source() {
        assert!(source_ids().contains(&"docker"));
    }

    #[cfg(not(feature = "trigger-docker"))]
    #[test]
    fn the_catalogue_has_no_docker_source_without_the_feature() {
        assert!(!source_ids().contains(&"docker"));
    }
```

If clippy or rustc warns that `source_ids` is unused when the feature is off, that cannot happen because the second test uses it; do not add an allow.

6. Run `cd src-tauri && cargo fmt` once, then the acceptance commands. `Cargo.lock` changes by gaining a `trigger-docker` entry under `terminal-grid`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid the_catalogue_lists_the_docker_source` passes (1 test)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo test -p terminal-grid --no-default-features the_catalogue_has_no_docker_source_without_the_feature` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features --features trigger-docker` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- Changing any file under `src-tauri/crates/`. The crate is finished.
- `.github/workflows/ci.yml` (adding trigger-docker to the clippy line or a live test step). The architect decides that separately.
- The repo map and architecture table: T31.
- Any change to `TriggerService`, the commands, Settings or the frontend. The catalogue already reaches the UI.
- Fixing the app's existing clippy warnings.

## If stuck

- If `cargo check` says `unresolved import trigger_docker`, the `[dependencies]` line from step 3 is missing or has no `optional = true`; fix it.
- If `cargo test --workspace` fails inside `trigger-docker` only because no container engine is installed, set status blocked and name the test; the pure tests must not need an engine.
- If `Cargo.lock` is not updated because the build is offline (`--locked` or network errors), run `cd src-tauri && cargo metadata --offline > /dev/null`; the crate is a path dependency so it needs no download. If that fails too, set status blocked.
- If the clippy count grew, show the new warning lines in the Log; if they come from the new test code, fix them, otherwise set status blocked.
- If the `--no-default-features` test run fails to link or build for a reason unrelated to this card (for example tauri system libraries), run only the two `cargo check` commands and note the skipped test in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
