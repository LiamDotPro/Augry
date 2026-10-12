---
id: T35
epic: 011-trigger-github
title: Lint trigger-github in CI
status: done
depends_on: [T34]
size: S
model: haiku
files:
  - .github/workflows/ci.yml
  - plans/_context/repo-map.md
---

## Goal

CI lints `trigger-github` with `-D warnings` like the other trigger crates. CI does not run its live tests, because they need a signed-in `gh` and CI has none; the repo map says so.

## Read first

- `.github/workflows/ci.yml` the `Clippy (trigger crates)` step and the `Live file watching tests` step (read only; the live step stays as it is)
- `plans/_context/repo-map.md` the `.github/workflows/ci.yml` row and the `Rust lints` row

## Steps

1. In `.github/workflows/ci.yml`, change the clippy command line from

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings`

   to

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-github --all-targets -- -D warnings`

   If the line already lists other `-p trigger-...` crates (from another epic), add ` -p trigger-github` after the last one instead and keep the rest.

2. Do not add any step for the `trigger-github` live tests.

3. In `plans/_context/repo-map.md`:
   - In the `.github/workflows/ci.yml` row, change ``clippy with `-D warnings` on the three trigger crates`` to ``clippy with `-D warnings` on the trigger crates``, and change ``and the `trigger-fs` live tests`` to ``and the `trigger-fs` live tests (the `trigger-github` live tests are not run in CI: they need a signed-in `gh`)``.
   - In the `Rust lints` row, change ``CI lints only `-p trigger-core -p trigger-hub -p trigger-fs` `` to ``CI lints the trigger crates by name with `-p` ``. If the row already has that wording (from another epic), make no change to it.

## Acceptance

- [ ] `grep -c "\-p trigger-github --all-targets" .github/workflows/ci.yml` prints `1`
- [ ] `grep -c "trigger-github" .github/workflows/ci.yml` prints `1`
- [ ] `node -e "const y=require('js-yaml');y.load(require('fs').readFileSync('.github/workflows/ci.yml','utf8'))"` exits 0 (if `js-yaml` is not installed, rely on the two greps)
- [ ] `grep -c "not run in CI" plans/_context/repo-map.md` prints `1`
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/011-trigger-github/tasks/`

## Out of scope

- A live-test step for `trigger-github`, a GitHub token secret, or installing `gh` on runners.
- Any other CI change (matrix, caching, other crates' steps).

## If stuck

- If the clippy line in `ci.yml` does not match either form in step 1, set status blocked and quote the line.
- If the repo map wording in step 3 is not found exactly, search with `grep -n "ci.yml" plans/_context/repo-map.md`, make the equivalent edit on the line you find, and note it in the Log.
- If clippy reports a warning in `trigger-github`, set status blocked and paste the first warning in the Log; do not edit the crate.
- Anything else: set status blocked, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-13 haiku: built, acceptance passed
- 2026-10-13 checker: pass
