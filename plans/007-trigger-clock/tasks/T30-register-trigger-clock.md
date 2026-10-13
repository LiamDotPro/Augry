---
id: T30
epic: 007-trigger-clock
title: Register trigger-clock in the app behind a feature
status: done
depends_on: [T24]
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/src/triggers/mod.rs
---

## Goal

The app has a cargo feature `trigger-clock` (on by default) that pulls in the `trigger-clock` crate, and `sources()` registers `trigger_clock::ClockSource` behind it, so the trigger catalogue lists the `clock` source. A unit test proves it, and the app still builds with every source feature off.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 12
- `src-tauri/Cargo.toml:27-37` the `[features]` block and the first dependencies
- `src-tauri/src/triggers/mod.rs:29-39` `sources()`
- `src-tauri/src/triggers/mod.rs:246-252` the start of `mod tests` (its `use` lines)
- `src-tauri/crates/trigger-clock/src/lib.rs:151` `ClockSource` is a unit struct

## Steps

1. Baseline. Run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It must not grow.

2. In `src-tauri/Cargo.toml` replace the `[features]` block with exactly:

```toml
[features]
default = ["trigger-fs", "trigger-clock"]
trigger-fs = ["dep:trigger-fs"]
trigger-clock = ["dep:trigger-clock"]
```

3. In the same file, directly under the line `trigger-fs = { path = "crates/trigger-fs", optional = true }` in `[dependencies]` add:

```toml
trigger-clock = { path = "crates/trigger-clock", optional = true }
```

4. In `src-tauri/src/triggers/mod.rs`, in `sources()`, directly under the line `list.push(Box::new(trigger_fs::FsSource));` add:

```rust
    #[cfg(feature = "trigger-clock")]
    list.push(Box::new(trigger_clock::ClockSource));
```

Do not change anything else in the function.

5. In the same file, inside `mod tests`, add these tests directly after the `has_definition_looks_for_the_file` test (the one ending with `cleanup(&dir);` then `}`). `Source` is reached through `use super::*`.

```rust
    fn source_ids() -> Vec<&'static str> {
        sources().iter().map(|source| source.id()).collect()
    }

    #[cfg(feature = "trigger-clock")]
    #[test]
    fn the_catalogue_lists_the_clock_source() {
        assert!(source_ids().contains(&"clock"));
    }

    #[cfg(not(feature = "trigger-clock"))]
    #[test]
    fn the_catalogue_has_no_clock_source_without_the_feature() {
        assert!(!source_ids().contains(&"clock"));
    }
```

If `source_ids` is reported unused, do not add an allow; the second test uses it when the feature is off, the first when on.

6. Run `cd src-tauri && cargo fmt` once, then the acceptance commands. `Cargo.lock` gains a `trigger-clock` entry under `terminal-grid`; that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid the_catalogue_lists_the_clock_source` passes (1 test)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo test -p terminal-grid --no-default-features the_catalogue_has_no_clock_source_without_the_feature` passes
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features --features trigger-clock` exits 0
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints a number not larger than the baseline in the Log
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/007-trigger-clock/tasks/`

## Out of scope

- Changing any file under `src-tauri/crates/`. The crate is finished.
- `.github/workflows/ci.yml`: T32.
- The repo map and architecture table: T31.
- Any change to `TriggerService`, the commands, Settings or the frontend.
- Fixing the app's existing clippy warnings.

## If stuck

- If `cargo check` says `unresolved import trigger_clock`, the `[dependencies]` line from step 3 is missing or has no `optional = true`; fix it.
- If a `trigger-fs` line is not where step 3 or 4 says (another epic changed it), put the new line directly after the last `trigger-` line in that block and note it in the Log.
- If `Cargo.lock` is not updated because the build is offline, run `cd src-tauri && cargo metadata --offline > /dev/null`. If that fails too, set status blocked.
- If the clippy count grew, show the new warning lines in the Log; if they come from the new test code, fix them, otherwise set status blocked.
- If `cargo test --workspace` fails in a `trigger-clock` test, set status blocked and name the test.
- If the `--no-default-features` test run fails to link or build for a reason unrelated to this card (for example tauri system libraries), run only the two `cargo check` commands and note the skipped test in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Clippy baseline 7 warnings, after 7 (not grown). Cargo.lock gained `"trigger-clock"` under terminal-grid (expected, under files:). Ran cargo fmt once; it changed no unrelated lines.
- 2026-10-12 checker: pass
