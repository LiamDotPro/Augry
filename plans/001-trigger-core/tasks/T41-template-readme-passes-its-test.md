---
id: T41
epic: 001-trigger-core
title: Make a fresh trigger crate pass its own readme test
status: done
depends_on: [T29]
size: S
model: haiku
files:
  - plans/_templates/trigger-crate/README.md
---

## Goal

A crate made by `node plans/_tools/new-trigger-crate.mjs` passes `cargo test -p trigger-<name>` straight away. Today the template README has instruction text between the `<!-- kinds -->` markers, so `tests/readme.rs` fails until a human pastes the table.

## Read first

- `plans/_templates/trigger-crate/README.md` the file you edit, the "Kinds" section
- `plans/_templates/trigger-crate/src/lib.rs` the `KINDS` array, with the single kind `{{name}}.example`
- `plans/_templates/trigger-crate/tests/readme.rs` how the table is compared (trimmed text between the markers)
- `plans/001-trigger-core/tasks/T05-kinds-markdown.md` the exact table format

## Steps

1. In `plans/_templates/trigger-crate/README.md` find the Kinds section:

```markdown
## Kinds

<!-- kinds -->
Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-{{name}} --example readme` and paste the output here;
`tests/readme.rs` fails while this table is stale.
<!-- /kinds -->
```

and replace it with this. The instruction text moves above the marker; between the markers is exactly what `trigger_core::kinds_markdown` prints for the template's one kind (the `example` field is required, so it gets a `*` in the When column; the permission is none; the platforms are all three):

```markdown
## Kinds

Generated from `KINDS` in `src/lib.rs`. Refresh with
`cargo run -p trigger-{{name}} --example readme` and paste the output between
the markers; `tests/readme.rs` fails while this table is stale.

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| {{name}}.example | Example | macOS, Windows, Linux | none | example* | example |
<!-- /kinds -->
```

2. Change nothing else in the template. Do not touch `src/lib.rs`.

3. Prove it with a throwaway crate (steps 3 to 6 are the acceptance commands, run them in order):

```
node plans/_tools/new-trigger-crate.mjs zz-test "A throwaway crate to prove the template"
cd src-tauri && cargo test -p trigger-zz-test
```

4. Run `node plans/_tools/check-crates.mjs` and check it exits 0. Warnings about template text in the new crate are expected.

5. Delete the throwaway crate and restore the lock file, so the repo is left as it was: `rm -rf src-tauri/crates/trigger-zz-test && git checkout -- src-tauri/Cargo.lock`.

6. Run `git status --porcelain`. It must list only the template README.

## Acceptance

- [ ] `node plans/_tools/new-trigger-crate.mjs zz-test "A throwaway crate to prove the template"` prints `Created` and the path
- [ ] `cd src-tauri && cargo test -p trigger-zz-test` passes, including the test `readme_kinds_table_is_current`
- [ ] `cd src-tauri && cargo run -p trigger-zz-test --example readme` prints the same table as the one between the markers in `src-tauri/crates/trigger-zz-test/README.md`
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (run while the throwaway crate still exists)
- [ ] After `rm -rf src-tauri/crates/trigger-zz-test && git checkout -- src-tauri/Cargo.lock`: `ls src-tauri/crates` does not list `trigger-zz-test`
- [ ] `git status --porcelain` lists only `plans/_templates/trigger-crate/README.md`

## Out of scope

- Changing `new-trigger-crate.mjs` to run cargo. The table is static on purpose.
- Changing the kind in the template's `src/lib.rs`, or `kinds_markdown`.
- Editing `trigger-fs` or its README.
- Adding other example text to the template's other sections.

## If stuck

- If `cargo test -p trigger-zz-test` fails on `readme_kinds_table_is_current`, run `cd src-tauri && cargo run -p trigger-zz-test --example readme` and compare it with the README between the markers. If only whitespace or the `*` differs, the table in step 1 was typed wrongly: fix the template (not the generated crate), delete the throwaway crate and start step 3 again.
- If it fails to compile, the template is wrong against the real `trigger-core`. Set status blocked, quote the first error, and still run step 5 so the repo is left clean.
- If `trigger-zz-test` already exists, a previous attempt did not clean up: run step 5, then start again.
- If `git checkout -- src-tauri/Cargo.lock` says the path is not tracked, set status blocked.
- Always run step 5 before stopping, even when blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: BLOCKED at step 6 (git status check). Template README edited and all other acceptance passed: scaffold printed Created, cargo test -p trigger-zz-test passed incl. readme_kinds_table_is_current, example readme output matched README markers, check-crates exit 0, throwaway crate removed, Cargo.lock checked out. But git status --porcelain lists 3 files: plans/_templates/trigger-crate/README.md plus two modified files that were already in the worktree before this run: .github/workflows/ci.yml and plans/001-trigger-core/tasks/T40-ci-workspace-checks.md. Not reverted, since they are not mine. Needs the orchestrator to confirm these are expected or to move them out of this worktree.
- 2026-10-12 orchestrator: the scope failure was T40's uncommitted files (a git lock stopped its commit); T40 is now committed, so the card goes to the checker
- 2026-10-12 checker: pass
