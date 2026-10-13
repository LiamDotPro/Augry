---
id: T40
epic: 001-trigger-core
title: Run fmt, clippy, workspace tests, crate checks and live tests in CI
status: done
depends_on: [T29, T17]
size: S
model: haiku
files:
  - .github/workflows/ci.yml
---

## Goal

`.github/workflows/ci.yml` checks formatting, lints the three trigger crates, runs `cargo test --workspace`, runs `node plans/_tools/check-crates.mjs`, and runs the `trigger-fs` live tests, on macOS, Windows and Ubuntu. Today it runs a bare `cargo test`.

## Read first

- `.github/workflows/ci.yml` the whole file (it is short); you change two places in it
- `plans/001-trigger-core/research/R04-ci-live-tests.md` the section "ci.yml changes", the source of the YAML below
- `plans/001-trigger-core/epic.md` Decisions 17 and 20

## Steps

1. In `.github/workflows/ci.yml` find this line:

```yaml
      - uses: dtolnay/rust-toolchain@stable
```

and replace it with these three lines:

```yaml
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
```

2. Find the last step of the file, which is exactly:

```yaml
      - name: Core tests
        run: cargo test
        working-directory: src-tauri
```

and replace it with all of the following (the file ends after it). Keep the two space indentation, six spaces before each `- name`. The `check-crates` step has no `working-directory` because it runs from the repo root. Node is already set up by an earlier step.

```yaml
      - name: Format check
        run: cargo fmt --check
        working-directory: src-tauri

      - name: Clippy (trigger crates)
        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings
        working-directory: src-tauri

      - name: Core tests
        run: cargo test --workspace
        working-directory: src-tauri

      - name: Trigger crate rules
        run: node plans/_tools/check-crates.mjs

      - name: Inotify limits (Linux, for the log)
        if: runner.os == 'Linux'
        run: cat /proc/sys/fs/inotify/max_user_watches /proc/sys/fs/inotify/max_user_instances

      - name: Live file watching tests
        run: cargo test -p trigger-fs -- --ignored --test-threads=1
        working-directory: src-tauri
        timeout-minutes: 10
```

3. Change nothing else: not the matrix, not the triggers, not the Linux apt step, not the header comment.

## Acceptance

- [ ] `node -e "const y=require('js-yaml').load(require('fs').readFileSync('.github/workflows/ci.yml','utf8'));const s=y.jobs.test.steps;const names=s.map(x=>x.name||x.uses||x.run);console.log(names.join('\n'));if(!s.some(x=>x.run==='cargo test --workspace'))process.exit(1);if(s.some(x=>x.run==='cargo test'))process.exit(1)"` exits 0 and its list ends with the steps `Format check`, `Clippy (trigger crates)`, `Core tests`, `Trigger crate rules`, `Inotify limits (Linux, for the log)`, `Live file watching tests`
- [ ] `grep -n "components: rustfmt, clippy" .github/workflows/ci.yml` prints one line
- [ ] `grep -n "node plans/_tools/check-crates.mjs" .github/workflows/ci.yml` prints one line
- [ ] `grep -n "timeout-minutes: 10" .github/workflows/ci.yml` prints one line
- [ ] `grep -c "working-directory: src-tauri" .github/workflows/ci.yml` prints `4`
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Pushing, opening a pull request or running the workflow. This card is checked locally only.
- A retry action, a longer timeout, or a `sysctl` change for inotify (R04 says to add it only if the logged limit is under 8192).
- Gating fmt or clippy to one OS, changing the matrix, or touching `release.yml`.
- Fixing clippy warnings in the app crate: the clippy step names the three trigger crates on purpose.

## If stuck

- If `require('js-yaml')` fails with "Cannot find module", skip the first check and rely on the greps. Say so in the Log.
- If the `grep -c "working-directory: src-tauri"` count is not 4, a step was lost or duplicated: compare with step 2 (Format check, Clippy, Core tests and Live tests have one each; the `swatinem/rust-cache@v2` `workspaces: src-tauri` line is different and does not count).
- If `cargo fmt --check` fails, a file from an earlier card is not formatted. Do not reformat it here: set status blocked and name the file.
- If the live tests are never run locally by this card, that is expected; the first CI run is the proof.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (js-yaml check ok, fmt/clippy/workspace tests/check-crates exit 0; live tests not run locally, first CI run is the proof)
- 2026-10-12 checker: pass
