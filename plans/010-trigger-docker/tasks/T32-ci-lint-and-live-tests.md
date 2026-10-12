---
id: T32
epic: 010-trigger-docker
title: Lint trigger-docker and run its live tests in CI
status: todo
depends_on: [T31]
size: S
model: haiku
files:
  - .github/workflows/ci.yml
  - plans/_context/repo-map.md
---

## Goal

CI lints `trigger-docker` with `-D warnings` like the other trigger crates, and runs its live tests on every runner. The live tests skip with a message where no Docker or Podman engine answers (most runners), so they only exercise the engine where one exists.

## Read first

- `.github/workflows/ci.yml` the `Clippy (trigger crates)` step and the `Live file watching tests` step
- `plans/_context/repo-map.md` the `.github/workflows/ci.yml` row and the `Rust lints` row

## Steps

1. In `.github/workflows/ci.yml`, change the clippy command line from

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings`

   to

   `        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-docker --all-targets -- -D warnings`

   If the line already lists other `-p trigger-...` crates (from another epic), add ` -p trigger-docker` after the last one instead and keep the rest.

2. At the very end of the `steps:` list (after the `Live file watching tests` step and its `timeout-minutes` line), append this step with the same indentation as the steps above it:

```yaml
      - name: Live Docker tests (skip without an engine)
        run: cargo test -p trigger-docker -- --ignored --test-threads=1
        working-directory: src-tauri
        timeout-minutes: 10
```

3. In `plans/_context/repo-map.md`:
   - In the `.github/workflows/ci.yml` row, change `clippy with \`-D warnings\` on the three trigger crates` to `clippy with \`-D warnings\` on the trigger crates` and change `and the \`trigger-fs\` live tests` to `and the live tests of \`trigger-fs\` and \`trigger-docker\` (the Docker ones skip without an engine)`.
   - In the `Rust lints` row, change `CI lints only \`-p trigger-core -p trigger-hub -p trigger-fs\`` to `CI lints the trigger crates by name with \`-p\``.

## Acceptance

- [ ] `grep -c "\-p trigger-docker --all-targets" .github/workflows/ci.yml` prints `1`
- [ ] `grep -c "cargo test -p trigger-docker -- --ignored --test-threads=1" .github/workflows/ci.yml` prints `1`
- [ ] `node -e "const y=require('js-yaml');const d=y.load(require('fs').readFileSync('.github/workflows/ci.yml','utf8'));const s=d.jobs.test.steps.map(x=>x.name);if(!s.includes('Live Docker tests (skip without an engine)'))process.exit(1)"` exits 0 (if `js-yaml` is not installed, rely on the two greps)
- [ ] `grep -c "trigger-docker" plans/_context/repo-map.md` prints at least `2`
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `git status --porcelain` lists only paths under `files:` (and the card)

## Out of scope

- Any other CI change (matrix, caching, other crates' steps).
- Installing Docker on runners.

## If stuck

- If the clippy line in ci.yml does not match either form in step 1, set status blocked and quote the line.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
