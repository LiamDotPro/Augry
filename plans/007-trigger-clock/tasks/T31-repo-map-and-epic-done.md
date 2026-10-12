---
id: T31
epic: 007-trigger-clock
title: Add trigger-clock to the repo map and mark 007 done
status: todo
depends_on: [T30]
size: S
model: haiku
files:
  - plans/_context/repo-map.md
  - plans/_context/architecture.md
  - plans/007-trigger-clock/epic.md
---

## Goal

The repo map describes the `trigger-clock` crate and the feature that wires it in, the Epics table in `architecture.md` shows 007 as done, and the epic's `status:` line says `done`.

## Read first

- `plans/_context/repo-map.md:28` the `src-tauri/Cargo.toml` row
- `plans/_context/repo-map.md:39` the `trigger-fs` row
- `plans/_context/repo-map.md:61` the Commands table row for live tests
- `plans/_context/architecture.md:71-79` the Epics table
- `src-tauri/crates/trigger-clock/README.md` the Kinds table and Develop table

## Steps

1. In `plans/_context/repo-map.md`, in the `src-tauri/Cargo.toml` row, replace the text `Feature `trigger-fs` (on by default) wires in the file source.` with `Features `trigger-fs` and `trigger-clock` (both on by default) wire in the file and clock sources.`

2. In the same file, directly after the row that starts `| `src-tauri/crates/trigger-fs/` |`, add this row:

```
| `src-tauri/crates/trigger-clock/` | Fires on a cron schedule, a plain interval, the first use of the day, and before and after calendar feed events (ICS URL or file), with optional catch up after sleep. Kinds `clock.schedule`, `clock.interval`, `clock.daily-first-use`, `clock.event-starting`, `clock.event-ended`. `logic.rs` (pure `step` over a clock value), `feed.rs` (pure ICS expansion), `rule.rs`, `threads.rs` (the `trigger-clock` and `trigger-clock-feed` threads), `os.rs` (`UreqFetcher`, file reads, local zone) |
```

3. In the same file, add this row to the Commands table directly after the row that starts `| Live file watching tests |`:

```
| Live clock tests | `cd src-tauri && cargo test -p trigger-clock -- --ignored --test-threads=1` |
```

4. In `plans/_context/architecture.md`, in the Epics table, replace the row `| 003 to 015 | one crate per integration, all depending on 001 only | draft |` with these two rows, in this order:

```
| 007-trigger-clock | `trigger-clock`: schedule, interval, first use of the day, and calendar events, with catch up after sleep | done |
| 003 to 006, 008 to 015 | one crate per integration, all depending on 001 only | draft |
```

If the file already has a row starting `| 007`, set its Status cell to `done` instead and make no other change to the table. If the `003 to 015` row has already been split by another epic, remove `007, ` or `007` from the range text on that row if it is named there, and add only the 007 row.

5. In `plans/007-trigger-clock/epic.md`, change the front matter line `status: carding` (or whatever status it shows now, such as `building`) to `status: done`. Change nothing else in that file.

## Acceptance

- [ ] `grep -c "trigger-clock" plans/_context/repo-map.md` prints `4` or more
- [ ] `grep -n "007-trigger-clock" plans/_context/architecture.md` prints one line ending in `| done |`
- [ ] `grep -n "^status:" plans/007-trigger-clock/epic.md` prints `status: done`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 007-trigger-clock` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/007-trigger-clock/tasks/`

## Out of scope

- Editing the "Process triggers (epic 001)" section of the repo map, the CI row, the Rust lints row (T32), or any code.
- Changing other epics' rows in the Epics table.
- Editing the epic body, Decisions or research.

## If stuck

- If a row text from steps 1 or 4 is not found exactly, search with `grep -n "trigger-fs" plans/_context/repo-map.md` or `grep -n "003 to" plans/_context/architecture.md`, make the equivalent edit on the line you find, and note it in the Log.
- If `check-crates.mjs` fails, it is not about these files; set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
