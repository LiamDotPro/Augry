---
id: T00
epic: NNN-slug
title: Imperative, under 60 characters
status: todo            # todo | in-progress | done | blocked
depends_on: []          # card ids in this epic, or "NNN-slug/TNN" for another epic
size: S                 # S (< ~150 changed lines) | M (< ~400)
model: haiku            # haiku by default; sonnet only when the card says why
files:                  # every path this card may create, modify or move
  - src-tauri/crates/trigger-fs/src/lib.rs
---

## Goal

One or two sentences: what exists after this card that did not before.

## Read first

At most about six entries. Path, optional line range, and why.

- `plans/_context/conventions.md`
- `src-tauri/src/lib.rs:118-160` the command list this card adds a command to

## Steps

Numbered, concrete, in order. Give exact commands. Give code wherever the shape
is not obvious (types, signatures, config, SQL, JSON). A step that says
"implement X" without saying what X looks like is not finished.

1. ...
2. ...

## Acceptance

Every check is a command and its expected result. Run from the repo root
unless stated.

- [ ] `cd src-tauri && cargo test -p trigger-fs` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `npm test` passes
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

Things a builder might reasonably be tempted to do here, and must not.

- ...

## If stuck

Situations the writer can foresee, with what to do. Anything not listed: set
`status: blocked`, add a Log line saying what you saw, and stop.

- If `X` fails with `Y`, do `Z`.

## Log

<!-- one line per attempt: date, model, result, short note -->
