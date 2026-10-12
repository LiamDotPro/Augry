---
id: T32
epic: 007-trigger-clock
title: Lint trigger-clock and run its live tests in CI
status: done
depends_on: [T31]
size: S
model: haiku
files:
  - .github/workflows/ci.yml
  - plans/_context/repo-map.md
---

## Goal

CI lints `trigger-clock` with `-D warnings` like the other trigger crates, and runs its ignored live tests on every runner.

## Read first

- `.github/workflows/ci.yml` the `Clippy (trigger crates)` step and the `Live file watching tests` step at the end
- `plans/_context/repo-map.md` the `.github/workflows/ci.yml` row and the `Rust lints` row

## Steps

1. In `.github/workflows/ci.yml`, change the clippy command line from

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings`

   to

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-clock --all-targets -- -D warnings`

   If the line already lists other `-p trigger-...` crates (from another epic), add ` -p trigger-clock` after the last one instead and keep the rest.

2. At the very end of the `steps:` list (after the last step and its `timeout-minutes` line), append this step with the same indentation as the steps above it:

```yaml
      - name: Live clock tests
        run: cargo test -p trigger-clock -- --ignored --test-threads=1
        working-directory: src-tauri
        timeout-minutes: 10
```

3. In `plans/_context/repo-map.md`:
   - In the `.github/workflows/ci.yml` row, change `clippy with \`-D warnings\` on the three trigger crates` to `clippy with \`-D warnings\` on the trigger crates` and change `and the \`trigger-fs\` live tests` to `and the live tests of \`trigger-fs\` and \`trigger-clock\``.
   - In the `Rust lints` row, change `CI lints only \`-p trigger-core -p trigger-hub -p trigger-fs\`` to `CI lints the trigger crates by name with \`-p\``.

   If another epic already made one of these edits, keep its wording and only add `trigger-clock` where the crates are named.

## Acceptance

- [ ] `grep -c "\-p trigger-clock --all-targets" .github/workflows/ci.yml` prints `1`
- [ ] `grep -c "cargo test -p trigger-clock -- --ignored --test-threads=1" .github/workflows/ci.yml` prints `1`
- [ ] `node -e "const y=require('js-yaml');const d=y.load(require('fs').readFileSync('.github/workflows/ci.yml','utf8'));const s=d.jobs.test.steps.map(x=>x.name);if(!s.includes('Live clock tests'))process.exit(1)"` exits 0 (if `js-yaml` is not installed, rely on the two greps)
- [ ] `grep -c "trigger-clock" plans/_context/repo-map.md` prints `4` (the three rows from T31 plus the ci.yml row)
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `git status --porcelain` lists only paths under `files:` (and the card)

## Out of scope

- Any other CI change (matrix, caching, other crates' steps).
- Making the live tests faster or changing them.

## If stuck

- If the clippy line in ci.yml does not match either form in step 1, set status blocked and quote the line.
- If the clippy command fails on code in `trigger-clock`, set status blocked and paste the first error in the Log; do not edit the crate.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: blocked. Steps 1-3 done. Acceptance failed: `grep -c "trigger-clock" plans/_context/repo-map.md` prints 4, card requires at least 5. The steps only add trigger-clock to the ci.yml row (line 49), and the Rust lints row now reads "CI lints the trigger crates by name with `-p`" with no crate name, so the steps as written yield 4 (lines 28, 40, 49, 63). Not improvising a wording change; card needs the Rust lints row or the count fixed. Other checks passed: both ci.yml greps print 1, js-yaml check exits 0, clippy with -p trigger-clock exits 0.
- 2026-10-12 architect: the 5 assumed T31's old miscount; 4 is right. Reviewed the ci.yml and repo-map diff myself; it matches the card.
- 2026-10-12 checker: pass
