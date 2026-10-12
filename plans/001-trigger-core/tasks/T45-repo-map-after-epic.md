---
id: T45
epic: 001-trigger-core
title: Update the repo map for the workspace and the trigger crates
status: todo
depends_on: [T39, T40]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
---

## Goal

`plans/_context/repo-map.md` is true for the commit after epic 001: it describes the workspace, the three crates, `src-tauri/src/triggers/`, the new commands and the `trigger://fired` event, and it no longer says there is no crates folder.

## Read first

- `plans/_context/repo-map.md` the whole file
- `plans/001-trigger-core/tasks/T36-commands-hooks-contract-docs.md` the command names and the event
- `plans/001-trigger-core/tasks/T37-reducer-notice-and-helpers.md` and `T38-provider-actions-and-notice-toast.md` the frontend file names
- `plans/_context/conventions.md` the "Writing" section

## Steps

1. Under `## Today (before epic 001)` change the heading line to `## Today`. Change nothing else in that section intro.

2. In the table of that section, find the row that starts `| `src-tauri/Cargo.toml` |` and replace the whole row with:

```
| `src-tauri/Cargo.toml` | The app crate `terminal-grid` (lib `terminal_grid_lib`), also the root of a Cargo workspace whose members are `crates/*`. `[workspace.dependencies]` holds the shared versions. Feature `trigger-fs` (on by default) wires in the file source. Rust 2021, stable toolchain |
```

3. Find the row that starts `| `.github/workflows/ci.yml` |` and replace the whole row with:

```
| `.github/workflows/ci.yml` | On every pull request and push to main, on macOS, Windows and Ubuntu: `npm test`, `npm run build`, `cargo fmt --check`, clippy with `-D warnings` on the three trigger crates, `cargo test --workspace`, `node plans/_tools/check-crates.mjs`, and the `trigger-fs` live tests |
```

4. In the `src/` rows of the table, directly after the row that starts `| `src/lib/` |`, add these two rows:

```
| `src/lib/triggers.ts` | Pure helpers for process triggers: the toast text for a fired trigger (`firedNotice`) and the one line status of a process (`processStatusLine`) |
| `src/ipc/types.ts` (triggers) | `TriggersStatus`, `TriggerSourceInfo`, `TriggerFiredEvent` and friends; the `triggers_*` entries in `Commands`; `EVENTS.triggerFired` |
```

5. Directly after the row that starts `| `src-tauri/src/state/` |`, add these rows:

```
| `src-tauri/src/triggers/` | `mod.rs` (`TriggerService` in `AppState`, `sources()`, `refresh()`, the forward thread, the four commands), `store.rs` (definition files in `<config dir>/processes/`), `variables.rs` (`${home}`, `${downloads}`, `${notes.root}`, `${screenshots}`, `${repo.root}`), `rules.rs` (the pure planner), `status.rs` (status and `trigger://fired` JSON) |
| `src-tauri/crates/trigger-core/` | The contract (`Source`, `Rule`, `Event`, `KindInfo`, `kinds_markdown`) and `testkit` (`RecordingSink`, `FixedClock`, `FakeSource`) |
| `src-tauri/crates/trigger-hub/` | `Hub`: catalogue, `check`, `apply`, `publish`, `subscribe`, the two second cool down, the `trigger-hub` thread |
| `src-tauri/crates/trigger-fs/` | The reference source. Kinds `fs.file-saved`, `fs.file-created`, `fs.file-removed`, `fs.file-renamed`, `fs.download-finished`, over `notify` |
```

6. In the Commands table, replace the Rust tests row:

```
| Rust tests | `cd src-tauri && cargo test` (after epic 001: `cargo test --workspace`) |
```

with:

```
| Rust tests | `cd src-tauri && cargo test --workspace` |
| Live file watching tests | `cd src-tauri && cargo test -p trigger-fs -- --ignored --test-threads=1` |
```

and in the Rust lints row add after the closing text `new crates must be clean` a sentence: the row becomes

```
| Rust lints | `cd src-tauri && cargo clippy --all-targets -- -D warnings` (the app crate has a few existing warnings; new crates must be clean; CI lints only `-p trigger-core -p trigger-hub -p trigger-fs`) |
```

7. Under `### Things a card must not assume`, delete this bullet (both lines):

```
- There is no `src-tauri/crates/` folder and no Cargo workspace until epic 001.
```

8. Replace the whole section that starts `## After epic 001` (the heading and the paragraph under it, to the end of the file) with:

```markdown
## Process triggers (epic 001)

How a trigger gets from a file on disk to a toast:

1. A definition file `<app config dir>/processes/<id>.json` has a `triggers` list. Settings, under "Process triggers (experimental)", switches triggers on (`Config.triggers.enabled`) and arms processes (`Config.triggers.armed`, set by `triggers_arm`).
2. `TriggerService::refresh()` reads the files, expands variables, and gives the rules to `Hub::apply`. Rule ids are `<processId>/<triggerId>`, with `#<n>` added when `${repo.root}` fans out. It runs on start, after config saves, and after a terminal is created, closed or changes directory.
3. `trigger-hub` starts the source that owns each rule (`fs.*` goes to `trigger-fs`), applies the two second cool down, and publishes `Fired`.
4. The `trigger-forward` thread logs one line (ids and kind only) and emits `trigger://fired`. The reducer shows a neutral toast for six seconds. Running a process on a trigger is a later epic.

IPC added by the epic: commands `triggers_catalogue`, `triggers_check`, `triggers_status`, `triggers_arm`, and the event `trigger://fired`. `triggers_status` returns, per rule, the state `active`, `rejected`, `sourceFailed` or `skipped`.

To add a source: `node plans/_tools/new-trigger-crate.mjs <name> "<summary>"`, fill in the crate, then add its optional dependency and feature to `src-tauri/Cargo.toml` and one line to `sources()` in `src-tauri/src/triggers/mod.rs`. `node plans/_tools/check-crates.mjs` must exit 0.
```

9. Run the acceptance commands.

## Acceptance

- [ ] `grep -n "After epic 001" plans/_context/repo-map.md` prints nothing
- [ ] `grep -n "no \`src-tauri/crates/\` folder" plans/_context/repo-map.md` prints nothing
- [ ] `grep -n "cargo test --workspace" plans/_context/repo-map.md` prints at least one line, and `grep -n "after epic 001" plans/_context/repo-map.md` prints nothing
- [ ] `grep -n "## Process triggers (epic 001)" plans/_context/repo-map.md` prints one line
- [ ] `grep -c "trigger://fired\|triggers_status\|trigger-hub\|trigger-fs\|TriggerService" plans/_context/repo-map.md` prints at least `6`
- [ ] `node plans/_tools/lint-cards.mjs 001-trigger-core` exits with no errors for this card
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Any other file in `plans/_context/` (the architecture table is T46).
- Rewriting rows that are still true.
- Changing code or docs.

## If stuck

- If a row to replace is not found by its starting text, the file changed. Add the new row after the nearest row of the same area and say so in the Log.
- If `src/lib/triggers.ts` or a name in the new rows does not exist when you check (`ls src/lib/triggers.ts`; `grep -n triggerFired src/ipc/types.ts`), set status blocked and name it; do not write a row for something that is not there.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
