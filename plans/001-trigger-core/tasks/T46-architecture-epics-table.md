---
id: T46
epic: 001-trigger-core
title: Mark epic 001 done in the architecture table
status: done
depends_on: [T45]
size: S
model: haiku
files:
  - plans/_context/architecture.md
---

## Goal

The Epics table in `plans/_context/architecture.md` shows epic 001 as `done`.

## Read first

- `plans/_context/architecture.md` the "Epics" table near the end

## Steps

1. Find this row:

```
| 001-trigger-core | workspace, contract, hub, `trigger-fs`, app wiring, Settings switch | draft |
```

and replace it with:

```
| 001-trigger-core | workspace, contract, hub, `trigger-fs`, app wiring, Settings switch | done |
```

2. Change nothing else in the file.

## Acceptance

- [ ] `grep -n "001-trigger-core" plans/_context/architecture.md` prints one line, ending in `| done |`
- [ ] `grep -c "| draft |" plans/_context/architecture.md` prints `2` (the 002 and 003 to 015 rows)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Editing the principles, the diagram or the other rows.
- Marking other epics.

## If stuck

- If the row is not found, search `grep -n "001" plans/_context/architecture.md`. If the status is already `done`, change nothing and note it in the Log. Otherwise set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
