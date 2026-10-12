---
id: T44
epic: 001-trigger-core
title: Document trigger testing in design section 14
status: done
depends_on: [T43]
size: S
model: haiku
files:
  - docs/technical-design.md
---

## Goal

Section 14 (Testing) says how the trigger crates and the app side of triggers are tested, and how the live tests run.

## Read first

- `docs/technical-design.md:649-673` section 14
- `plans/001-trigger-core/epic.md` Decision 17 (the CI steps and the live tests)
- `.github/workflows/ci.yml` the steps T40 added
- `plans/_context/conventions.md` the Rust section on tests

## Steps

1. In section 14, find the line `Unit tests in TypeScript:`. Directly above it, with a blank line before and after, add:

````markdown
Unit tests for process triggers, all run by `cargo test --workspace` offline and without a display:

- `trigger-core`: the JSON the frontend sees (`Platforms`, `Permission`, `Event.at`), `kinds_markdown` against an exact table, the test kit (`RecordingSink`, `FixedClock`, `FakeSource`).
- `trigger-hub`: apply, restart only the sources whose rules changed, rejected rules, a source whose start fails, the two second cool down and the `coalesced` count, using `FakeSource`.
- `trigger-fs`: `rule.rs` parsing and `logic.rs` (globs and default ignores, settle and debounce, rename pairing, download finished) as pure tests over observed signals. No timers and no disk.
- Each source crate's `tests/readme.rs` fails when the Kinds table in its README is stale. Refresh it with `cargo run -p trigger-<name> --example readme`.
- The app side (`src-tauri/src/triggers/`): definition parsing, variable expansion with string level tests for `${screenshots}` on every OS, the rule planner, the status and `trigger://fired` JSON, and `TriggerService::refresh` applying only when the rules change, using a hub with no sources.

Live tests, which watch the real file system, are `#[ignore]` in `tests/live.rs` of a crate and run with `cargo test -p trigger-fs -- --ignored --test-threads=1`. Each uses its own canonicalised temp directory, waits 400 ms after the watcher starts, and waits for events with a 10 second ceiling rather than a fixed sleep; a negative check first waits for an event on a sentinel file. CI runs them on macOS, Windows and Linux after the other checks.

CI (`.github/workflows/ci.yml`) runs on each of the three systems: `npm test`, `npm run build`, `cargo fmt --check`, `cargo clippy` with `-D warnings` on the three trigger crates, `cargo test --workspace`, `node plans/_tools/check-crates.mjs` (crate isolation, README sections, file sizes) and the live tests.

Manual check for triggers: in Settings switch on "Process triggers (experimental)", put a definition file in the `processes/` folder with a `fs.file-saved` trigger on a folder, tick "Armed", save a file in that folder, and confirm the toast "Trigger fs.file-saved fired for <name>" appears within a few seconds.
````

2. Run the acceptance commands.

## Acceptance

- [ ] `grep -n "Unit tests for process triggers" docs/technical-design.md` prints one line
- [ ] `grep -n "cargo test -p trigger-fs -- --ignored --test-threads=1" docs/technical-design.md` prints one line
- [ ] `grep -n "check-crates.mjs" docs/technical-design.md` prints at least two lines (this card and T42)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Editing the existing Windows manual checklist.
- Changing CI or any code.
- Documenting tests that do not exist in this epic.

## If stuck

- If `Unit tests in TypeScript:` is not found, put the block at the end of the Rust unit test list in section 14 and say so in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Anchor "Unit tests in TypeScript:" found at line 732 (section 14 is at 723, not 649-673 as the card Read-first says; located by heading). Block inserted with blank lines around it. Card depends_on T43 was not checked.
- 2026-10-12 checker: pass
