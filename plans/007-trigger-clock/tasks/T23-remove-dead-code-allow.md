---
id: T23
epic: 007-trigger-clock
title: Remove the temporary dead_code allow and stale comments
status: done
depends_on: [T22]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/lib.rs
  - src-tauri/crates/trigger-clock/src/logic.rs
---

## Goal

Everything in the crate is used, so the temporary `#![allow(dead_code)]` in `lib.rs` is gone and the crate builds without warnings. One comment in `logic.rs` that said "unused until" is made true. After this card no file has template or temporary text left.

## Read first

- `src-tauri/crates/trigger-clock/src/lib.rs:1-20` the module comment and the temporary allow
- `src-tauri/crates/trigger-clock/src/logic.rs:50-65` the comment on `Occurrence`
- `plans/_tools/check-crates.mjs:70-76` the text that counts as "template code" (a warning, not an error)

## Steps

1. In `src-tauri/crates/trigger-clock/src/lib.rs`, find these three lines

```rust
// Temporary: nothing calls the logic until the glue card wires the thread.
// That card removes this line.
#![allow(dead_code)]
```

and delete them together with the one blank line directly below them. The next line of the file is then `mod feed;`.

2. In `src-tauri/crates/trigger-clock/src/logic.rs`, find the two comment lines

```rust
/// One occurrence of a calendar event inside the window the feed thread
/// expanded. Unused until the event kinds are added to `step`.
```

and replace them with

```rust
/// One occurrence of a calendar event inside the window the feed thread
/// expanded. `events::fire_events` reads these from `State::feeds`.
```

3. Run `cd src-tauri && touch crates/trigger-clock/src/lib.rs && cargo build -p trigger-clock`. It must print no `warning:` lines. If it does, use If stuck.

4. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `grep -rc "allow(dead_code)" src-tauri/crates/trigger-clock/src` prints `0` for every file
- [ ] `cd src-tauri && touch crates/trigger-clock/src/lib.rs && cargo build -p trigger-clock 2>&1 | grep -c "^warning"` prints `0`
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/check-crates.mjs | grep -c "warn  trigger-clock"` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Fixing anything the compiler reports by deleting or rewriting code in other files. Report it instead (see If stuck).
- Editing `README.md`: card T24.
- Changing any `pub` item, constant or test.

## If stuck

- If step 3 prints a `warning: ... is never used` (or never read, constructed or called), do not delete or change the code. Set `status: blocked` and paste each warning in the Log with its file and line. A later pass decides whether the item goes or gets a use.
- If clippy reports anything after the allow is gone, do the same: set `status: blocked` and paste it.
- If `check-crates` still prints a `warn  trigger-clock: ... template` line, run `grep -rn "Template: remove\|not implemented yet" src-tauri/crates/trigger-clock` and write the matching file and line in the Log, then set `status: blocked`.
- If a find text in step 1 or 2 is not there, an earlier card was built differently from its text. Set `status: blocked` and quote the lines around where it should be.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
