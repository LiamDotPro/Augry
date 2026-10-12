---
id: T01
epic: 007-trigger-clock
title: Scaffold the trigger-clock crate and add its dependencies
status: done
depends_on: [001-trigger-core/T41]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/Cargo.toml
  - src-tauri/crates/trigger-clock/README.md
  - src-tauri/crates/trigger-clock/examples/readme.rs
  - src-tauri/crates/trigger-clock/src/lib.rs
  - src-tauri/crates/trigger-clock/src/rule.rs
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/os.rs
  - src-tauri/crates/trigger-clock/tests/live.rs
  - src-tauri/crates/trigger-clock/tests/readme.rs
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
---

## Goal

`src-tauri/crates/trigger-clock` exists, made by the scaffold script from the template, with `croner`, `chrono` and `globset` as dependencies and `chrono-tz` as a dev dependency. It builds and its template tests pass. Nothing is `clock` specific yet.

## Read first

- `plans/_templates/trigger-crate/README.md` the file the script copies
- `plans/_tools/new-trigger-crate.mjs` what the script creates
- `src-tauri/Cargo.toml:9-17` the `[workspace.dependencies]` block you add three lines to
- `plans/007-trigger-clock/epic.md` Decision 6 (dependencies and versions)
- `plans/007-trigger-clock/research/R01-cron.md` why `croner` and how it is used

## Steps

1. Check the crate does not exist yet: `ls src-tauri/crates`. It lists `trigger-core`, `trigger-fs` and `trigger-hub`, not `trigger-clock`. If `trigger-clock` is there, use If stuck.

2. From the repo root run exactly:

```
node plans/_tools/new-trigger-crate.mjs clock "Fires on a schedule, an interval, the first use of the day, and around calendar events"
```

It prints `Created .../src-tauri/crates/trigger-clock` and the nine files listed under `files:` exist under it. The crate needs no entry in the workspace: `members = ["crates/*"]` picks it up. Do not register it in the app.

3. Edit `src-tauri/Cargo.toml`. In the `[workspace.dependencies]` block, directly under the line `globset = "0.4.20"`, add these three lines:

```toml
croner = "4.0.1"
chrono = { version = "0.4", default-features = false, features = ["std", "clock"] }
chrono-tz = "0.10"
```

4. Edit `src-tauri/crates/trigger-clock/Cargo.toml`. Directly under the line `serde_json = { workspace = true }` add:

```toml
globset = { workspace = true }
croner = { workspace = true }
chrono = { workspace = true }
```

and add this at the very end of the file, after a blank line:

```toml

[dev-dependencies]
chrono-tz = { workspace = true }
```

5. Run the acceptance commands. The crate has only the template's example kind and code now; that is expected. `Cargo.lock` gains `croner`, `derive_builder`, `strum` and `chrono-tz` entries; that is expected.

## Acceptance

- [ ] `ls src-tauri/crates/trigger-clock/src src-tauri/crates/trigger-clock/tests` lists `lib.rs logic.rs os.rs rule.rs` and `live.rs readme.rs`
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 3 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `grep -c "croner\|chrono\|globset" src-tauri/crates/trigger-clock/Cargo.toml` prints `4`
- [ ] `grep -n "^croner\|^chrono\|^chrono-tz" src-tauri/Cargo.toml` prints 3 lines
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing any real code: kinds, rule fields, logic. Later cards do that.
- Adding `calcard`, `ureq` or `iana-time-zone` (slice B).
- Registering the crate in the app (`src-tauri/src/triggers`, the app's `[features]`): slice D.
- Editing anything under `plans/_templates/`.
- Upgrading any other dependency in `Cargo.lock`.

## If stuck

- If `src-tauri/crates/trigger-clock` already exists, set `status: blocked` and say so. Do not delete or overwrite it.
- If the build says a workspace dependency is not found, the line in step 3 is missing or misspelt. Fix the line; do not write a version into the crate's own `Cargo.toml`.
- If cargo cannot download `croner` (network), set `status: blocked` and quote the error.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
