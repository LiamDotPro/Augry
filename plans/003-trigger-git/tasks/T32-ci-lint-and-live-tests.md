---
id: T32
epic: 003-trigger-git
title: Lint trigger-git and run its live tests in CI
status: done
depends_on: [T31]
size: S
model: haiku
files:
  - .github/workflows/ci.yml
  - plans/_context/repo-map.md
---

## Goal

CI lints `trigger-git` with `-D warnings` like the other trigger crates, and runs its live tests on every runner. Git is present on the CI runners, so the live tests run for real.

## Read first

- `.github/workflows/ci.yml:56-58` the `Clippy (trigger crates)` step
- `.github/workflows/ci.yml:70-74` the `Live file watching tests` step
- `plans/_context/repo-map.md:48` the `.github/workflows/ci.yml` row and `:63` the `Rust lints` row

## Steps

1. In `.github/workflows/ci.yml`, change the clippy command line from

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings`

   to

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-git --all-targets -- -D warnings`

   If the line already lists other `-p trigger-...` crates (from another epic), add ` -p trigger-git` after the last one instead and keep the rest.

2. At the very end of the `steps:` list (after the last step and its `timeout-minutes` line, which today is the `Live file watching tests` step), append this step with the same indentation as the steps above it:

```yaml
      - name: Live git tests
        run: cargo test -p trigger-git -- --ignored --test-threads=1
        working-directory: src-tauri
        timeout-minutes: 10
```

3. In `plans/_context/repo-map.md`:
   - In the `.github/workflows/ci.yml` row, change `clippy with \`-D warnings\` on the three trigger crates` to `clippy with \`-D warnings\` on the trigger crates` (skip if already so) and change `and the \`trigger-fs\` live tests` to `and the live tests of \`trigger-fs\` and \`trigger-git\``. If another epic already changed that phrase to list several crates, add \`trigger-git\` to its list instead.
   - In the `Rust lints` row, change `CI lints only \`-p trigger-core -p trigger-hub -p trigger-fs\`` to `CI lints the trigger crates by name with \`-p\`` (skip if already so).

## Acceptance

- [ ] `grep -c "\-p trigger-git --all-targets" .github/workflows/ci.yml` prints `1`
- [ ] `grep -c "cargo test -p trigger-git -- --ignored --test-threads=1" .github/workflows/ci.yml` prints `1`
- [ ] `node -e "const y=require('js-yaml');const d=y.load(require('fs').readFileSync('.github/workflows/ci.yml','utf8'));const s=Object.values(d.jobs).flatMap(j=>j.steps.map(x=>x.name));if(!s.includes('Live git tests'))process.exit(1)"` exits 0 (if `js-yaml` is not installed, rely on the two greps)
- [ ] `grep -c "trigger-git" plans/_context/repo-map.md` prints `4` (the three rows from T31 plus the ci.yml row)
- [ ] `grep -c "CI lints only" plans/_context/repo-map.md` prints `0`
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `git status --porcelain` lists only paths under `files:` (and the cards)

## Out of scope

- Any other CI change (matrix, caching, other crates' steps).
- Installing git on runners; it is already there.
- Any code change.

## If stuck

- If the clippy line in ci.yml does not match either form in step 1, set status blocked and quote the line.
- If the repo-map phrases in step 3 are not found exactly, find the row with `grep -n "ci.yml\|Rust lints" plans/_context/repo-map.md`, make the equivalent edit, and note it in the Log.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
