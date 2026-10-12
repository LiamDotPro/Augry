---
id: T08
epic: 001-trigger-core
title: Finish the trigger-core README and run the full check
status: todo
depends_on: [T09]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-core/README.md
---

## Goal

`trigger-core` is complete: its README describes the contract and the test kit as they now exist, and the whole workspace builds, tests and lints clean. Slices B and C build on this card.

## Read first

- `src-tauri/crates/trigger-core/README.md` the file you replace (T01)
- `src-tauri/crates/trigger-core/src/lib.rs` and `src-tauri/crates/trigger-core/src/testkit.rs` to check the README against the code

## Steps

1. Replace the whole of `src-tauri/crates/trigger-core/README.md` with this text. Keep the `## Develop` heading exactly.

````markdown
# trigger-core

The contract between trigger sources and whatever runs them, plus a small
test kit. A source crate depends on this crate only. Design:
`docs/process-triggers.md` Part 2.

## The contract

- `Source`: `id`, `kinds`, `availability`, `check(rule)` and
  `start(rules, sink) -> Handle`. Dropping the `Handle` stops the source.
- `Rule`: `{ id, kind, when }` where `when` is a map of JSON values.
- `Event`: `{ rule, kind, at, payload }`. `at` is whole milliseconds since the
  Unix epoch in JSON. `Sink` is a `std::sync::mpsc::Sender<Event>`.
- `KindInfo` and `FieldInfo` describe a kind: platforms (`Platforms`, a bit set
  that serialises as `["macos","windows","linux"]`), the permission it needs
  and its `when` and `payload` fields.
- `Availability`: `Available`, `Unsupported { reason }` or
  `NeedsPermission { permission, how }`.
- `RuleError` and `SourceError` are the two error types a source returns.
- `kinds_markdown(kinds)` renders the kinds table. Each source crate's
  `tests/readme.rs` checks its README contains it.

A unit test pins the JSON the frontend reads, so changing a field name or a
serde attribute fails a test.

## Test kit

`trigger_core::testkit`, always compiled:

- `RecordingSink`: give `tx.clone()` to `Source::start`, then
  `wait_for(kind, timeout)` or `drain()`.
- `rule(id, kind, json!({...}))`: builds a `Rule`.
- `Clock`, `SystemClock`, `FixedClock`: time that tests control.
- `FakeSource`: a source that records the rules it was started with, can be
  made to fail in `start` (`fail_start`) and fires events with `fire`.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-core`
- `cargo clippy -p trigger-core --all-targets -- -D warnings`
- `cargo fmt -p trigger-core --check`

From the repo root: `node plans/_tools/check-crates.mjs`.
````

2. Run every command in Acceptance. If `cargo fmt --check` or clippy reports a problem in `trigger-core`, that means an earlier card left it; stop and use If stuck.

## Acceptance

- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo test -p trigger-core` shows 4 `platforms_tests`, 5 `json_tests`, 2 `handle_tests`, 2 `markdown_tests`, 4 `recording_tests` and 4 `fake_tests`, all ok
- [ ] `cd src-tauri && cargo clippy -p trigger-core --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 and prints `1 crate(s), 0 error(s), 0 warning(s)`
- [ ] `git status --porcelain` lists only `src-tauri/crates/trigger-core/README.md`

## Out of scope

- Any code change. This card edits the README only.
- Fixing the app crate's own clippy warnings or formatting.
- Updating `plans/_context/repo-map.md`. Epic 001's last card does that.

## If stuck

- If a count in the second check differs, an earlier card is wrong or missing. Set status blocked and list the counts you saw.
- If `cargo fmt -p trigger-core --check` or clippy fails, set status blocked and quote the first error. Do not fix code in this card.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
