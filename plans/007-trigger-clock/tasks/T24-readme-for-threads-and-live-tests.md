---
id: T24
epic: 007-trigger-clock
title: Finish the clock README
status: todo
depends_on: [T23]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/README.md
---

## Goal

The README is finished: it says what the two threads do, the Develop table lists `threads.rs`, its tests and the live tests, how to run them is right, and the "Not finished yet" bullet is gone. `check-crates` reports no warnings for `trigger-clock`.

## Read first

- `src-tauri/crates/trigger-clock/README.md` the file you edit; T09 and T18 wrote the parts you change
- `src-tauri/crates/trigger-clock/src/threads.rs` what the threads do (read the module comment and `launch`)
- `plans/007-trigger-clock/epic.md` Decisions 7 and 9

## Steps

1. In `src-tauri/crates/trigger-clock/README.md`, under `## Platforms`, find the line

```
never trusts the length of a wait, only the wall clock.
```

and replace it with these lines (the sentence continues from the line above it, so the first line stays the same text):

```
never trusts the length of a wait, only the wall clock.

While rules are armed there are two threads. `trigger-clock` waits at most 30
seconds, reads the wall clock and fires what is due. `trigger-clock-feed`
exists only when a rule names a feed: it reads each feed when the source starts
and then every 5 minutes. Stopping the source stops both within a second.
```

2. In the Develop table, find the row that starts `| `src/lib.rs` |` and replace that one row with:

```
| `src/lib.rs` | `KINDS`, `ClockSource` and `start`, which parses the rules and starts the threads. Glue only |
| `src/threads.rs` | The two threads: `launch`, the clock loop and the feed loop. Joins the wall clock, the feeds and the pure logic to the sink |
| `src/threads/tests.rs` | Tests for the threads with a moved `FixedClock` and a fake `Fetcher`: no sleeping, no network |
```

3. In the same table, find the row that starts `| `tests/live.rs` |` and replace that one row with:

```
| `tests/live.rs` | `#[ignore]` tests with the real clock: an interval rule fires, and event rules fire from a feed file. About ten seconds in all |
```

4. In the command block under the table, find the line

```
cargo test -p trigger-clock -- --ignored          # live tests on this machine
```

and replace it with

```
cargo test -p trigger-clock -- --ignored --test-threads=1   # live tests on this machine, about 10 seconds
```

5. In `## Known limits`, find the bullet (T18 wrote it)

```
- Not finished yet: the threads that run the logic and fetch feeds, so `start`
  still returns unavailable.
```

and replace it with these bullets:

```
- A feed that cannot be read (no network, a wrong address, a file that is gone)
  keeps its last good copy and is tried again at the next 5 minute read. Nothing
  says why: feed addresses often hold a secret token, so the crate never logs
  them or the reason.
- A change to a feed is seen at the next 5 minute read, so an event added a
  minute before it starts can be missed.
- Stopping the source does not cancel a request already running. It ends by
  itself, at the latest at its 15 second timeout, and its answer is dropped.
```

6. Run `cd src-tauri && cargo test -p trigger-clock --test readme` to confirm the Kinds table is untouched.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock --test readme` passes (1 test)
- [ ] `grep -c "^## " src-tauri/crates/trigger-clock/README.md` prints `8`
- [ ] `grep -c "Not finished yet\|not implemented yet\|Replace with" src-tauri/crates/trigger-clock/README.md` prints `0`
- [ ] `grep -c "src/threads.rs" src-tauri/crates/trigger-clock/README.md` prints `1`
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes
- [ ] `cd src-tauri && cargo test -p trigger-clock -- --ignored --test-threads=1` passes (3 tests)
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/check-crates.mjs | grep -c "trigger-clock"` prints `0` (no warning or error mentions the crate)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Editing code or the Kinds table between the `<!-- kinds -->` markers.
- Registering the crate in the app or editing `plans/_context/repo-map.md`: slice D.
- Adding new `##` sections: the README must keep exactly eight.

## If stuck

- If a "find" text is not in the README, T09, T18 or this card's earlier steps were not applied as written. Set `status: blocked` and say which text.
- If the readme test fails with "Kinds table is stale", you changed something between the `<!-- kinds -->` markers. Undo that edit.
- If `check-crates` still mentions `trigger-clock`, paste its line in the Log and set `status: blocked`.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
