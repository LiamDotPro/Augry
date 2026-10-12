---
id: T17
epic: 001-trigger-core
title: Finish the trigger-hub README and run the full check
status: done
depends_on: [T16]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-hub/README.md
---

## Goal

`trigger-hub` is complete: its README describes the hub as it now behaves, and the whole workspace builds, tests and lints clean. Slice D builds on this card.

## Read first

- `src-tauri/crates/trigger-hub/README.md` the file you replace (T10)
- `src-tauri/crates/trigger-hub/src/lib.rs` to check the README against the code

## Steps

1. Replace the whole of `src-tauri/crates/trigger-hub/README.md` with this text. Keep the `## Develop` heading exactly.

````markdown
# trigger-hub

Runs trigger sources: keeps the rule registry, starts and stops sources so
that exactly the applied rules are live, cools down noisy rules and fans fired
events out to subscribers. It depends on `trigger-core` only. Design:
`docs/process-triggers.md` Part 2.

## What it does

- `Hub::new(sources)`: takes the sources. It starts one thread, named
  `trigger-hub`, that stops when the `Hub` is dropped. Every method takes
  `&self`, and `Hub` is `Send + Sync`.
- `catalogue()`: every source with its kinds and availability.
- `check(rule)`: the source that owns the rule asks whether the rule is valid.
  A rule belongs to the source whose id is the part of its kind before the
  first dot, so `fs.file-saved` belongs to `fs`.
- `apply(rules)`: makes exactly these rules live and returns a `RuleState`
  for every rule id passed in.
  - `Rejected`: no source owns the kind, or the source's `check` failed.
  - `SourceFailed`: the source is not available here, or its `start` failed.
    Other sources are not affected.
  - `Active`: the rule is running.
  - A source whose rules did not change is left running. A source whose
    rules changed is stopped and started again with all its rules. A source
    left with no rules is stopped.
- `publish(event)`: sends an event into the same channel the sources use, for
  triggers that come from inside the app.
- `subscribe()`: a receiver of `Fired { event, coalesced }`.
- `with_cool_down(duration)`: the default is two seconds.

## Cool down

Per rule, measured on `event.at`, not on a clock. An event inside the cool
down that follows the last event let through is dropped and counted. The next
`Fired` for that rule carries the count in `coalesced`, and the count starts
again from zero.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-hub`
- `cargo clippy -p trigger-hub --all-targets -- -D warnings`
- `cargo fmt -p trigger-hub --check`

From the repo root: `node plans/_tools/check-crates.mjs`.

Tests use `trigger_core::testkit::FakeSource`, wrapped by the helper in
`src/test_support.rs` so a test can read what the source recorded after the hub
took ownership of it.
````

2. Run every command in Acceptance. If `cargo fmt` or clippy reports a problem in `trigger-hub`, that means an earlier card left it; stop and use If stuck.

## Acceptance

- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo test -p trigger-hub` shows 6 `cool_down::tests`, 3 `catalogue_tests`, 7 `publish_tests`, 4 `check_tests`, 5 `apply_tests`, 8 `lifecycle_tests` and 5 `end_to_end_tests`, all ok
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-hub --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0, its summary says `0 error(s)`, and any `warn` line is only a size warning for a `tests.rs` file (the architect accepts test files over 400 lines; under 800 is fine)
- [ ] `git status --porcelain` lists only `src-tauri/crates/trigger-hub/README.md`

## Out of scope

- Any code change. This card edits the README only.
- Fixing the app crate's own clippy warnings or formatting.
- Updating `plans/_context/repo-map.md`. Epic 001's last card does that.

## If stuck

- If a count in the second check differs, an earlier card is wrong or missing. Set status blocked and list the counts you saw.
- If fmt or clippy fails, set status blocked and quote the first error. Do not fix code in this card.
- If `check-crates.mjs` prints a different crate count, `trigger-fs` may already exist from slice C; the expected error count is still 0 and the only warnings allowed are `tests.rs` size warnings. Set status blocked otherwise.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
