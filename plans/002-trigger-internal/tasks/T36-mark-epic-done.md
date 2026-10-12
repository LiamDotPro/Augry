---
id: T36
epic: 002-trigger-internal
title: Mark epic 002 done in the architecture table
status: todo
depends_on: [T34, T35]
size: S
model: haiku
files:
  - plans/_context/architecture.md
  - plans/002-trigger-internal/epic.md
---

## Goal

The Epics table in `plans/_context/architecture.md` shows 002 as done with a fuller description, and the epic's own front matter says `status: done`.

## Read first

- `plans/_context/architecture.md:71-79` the Epics table
- `plans/002-trigger-internal/epic.md:1-6` the front matter

## Steps

1. In `plans/_context/architecture.md`, find this row:

```
| 002-trigger-internal | triggers from inside the app | draft |
```

Replace it with:

```
| 002-trigger-internal | the `tg` source inside the app: agent finished, command failed or slow, output matched, bell, task reported, pane opened in a repo, worktrees, review staged, note saved, focus, app started; plus the shell integration fixes and the marks scanner it needed | done |
```

2. In `plans/002-trigger-internal/epic.md`, in the front matter on line 4, find:

```
status: carding
```

Replace it with:

```
status: done
```

Change nothing else in either file.

## Acceptance

- [ ] `grep -n "002-trigger-internal" plans/_context/architecture.md` prints one line ending in `| done |`
- [ ] `grep -c "^status: done" plans/002-trigger-internal/epic.md` prints `1`
- [ ] `grep -c "^status: carding" plans/002-trigger-internal/epic.md` prints `0`
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `node plans/_tools/lint-cards.mjs 002-trigger-internal` prints `0 error(s)`
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`

## Out of scope

- Changing other epics' rows in the table, or the epic's Decisions and Slices text.
- Marking any card done by hand.

## If stuck

- If the architecture row is not found exactly, run `grep -n "002" plans/_context/architecture.md`. If a row for 002 exists with other text, set its Status cell to `done` and make no other change, and note it in the Log.
- If `epic.md` already says `status: done`, make no change and note it in the Log.
- If `check-crates.mjs` fails, it is not about these files: set status blocked and paste its first error line in the Log.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
