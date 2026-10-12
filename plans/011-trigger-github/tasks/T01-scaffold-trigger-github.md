---
id: T01
epic: 011-trigger-github
title: Scaffold the trigger-github crate from the template
status: todo
depends_on: []
size: S
model: haiku
files:
  - src-tauri/crates/trigger-github/Cargo.toml
  - src-tauri/crates/trigger-github/README.md
  - src-tauri/crates/trigger-github/examples/readme.rs
  - src-tauri/crates/trigger-github/src/lib.rs
  - src-tauri/crates/trigger-github/src/rule.rs
  - src-tauri/crates/trigger-github/src/logic.rs
  - src-tauri/crates/trigger-github/src/os.rs
  - src-tauri/crates/trigger-github/tests/live.rs
  - src-tauri/crates/trigger-github/tests/readme.rs
  - src-tauri/Cargo.lock
  - plans/_templates/trigger-crate/Cargo.toml
  - plans/_templates/trigger-crate/README.md
  - plans/_templates/trigger-crate/examples/readme.rs
  - plans/_templates/trigger-crate/src/lib.rs
  - plans/_templates/trigger-crate/src/rule.rs
  - plans/_templates/trigger-crate/src/logic.rs
  - plans/_templates/trigger-crate/src/os.rs
  - plans/_templates/trigger-crate/tests/live.rs
  - plans/_templates/trigger-crate/tests/readme.rs
---

## Goal

`src-tauri/crates/trigger-github` exists, made by the scaffold script from the template, with the `globset` dependency declared, and it builds and tests clean against the real `trigger-core`. Nothing is GitHub specific yet. The crate is not registered in the app (that is slice D).

## Read first

- `plans/011-trigger-github/epic.md` Decisions 1, 3, 12 and 13 (what the crate is, its `when` fields, that registration comes last)
- `plans/_tools/new-trigger-crate.mjs` what the script creates and prints
- `plans/_templates/trigger-crate/README.md` the file the script copies; it already has a valid example Kinds table

## Steps

1. Check the crate does not exist yet: `ls src-tauri/crates`. It lists `trigger-core`, `trigger-fs` and `trigger-hub`, not `trigger-github`. If `trigger-github` is there, use If stuck.

2. From the repo root run exactly:

```
node plans/_tools/new-trigger-crate.mjs github "Fires on GitHub issues, reviews, CI runs, pull requests and releases for the signed in user"
```

It prints `Created .../src-tauri/crates/trigger-github` and the nine files `Cargo.toml`, `README.md`, `examples/readme.rs`, `src/lib.rs`, `src/rule.rs`, `src/logic.rs`, `src/os.rs`, `tests/live.rs` and `tests/readme.rs` now exist under it. Do not edit the generated files by hand except as the next step says. The crate needs no entry in the workspace: `members = ["crates/*"]` picks it up. Do not add it to `src-tauri/Cargo.toml` as a dependency or feature.

3. Edit `src-tauri/crates/trigger-github/Cargo.toml`. Directly under the line `serde_json = { workspace = true }` add this one line:

```toml
globset = { workspace = true }
```

Add nothing else. (The branch glob in a later card uses it. `serde_json` is already there.)

4. The generated README already has a valid Kinds table for the template's `github.example` kind, so nothing needs refreshing yet. Card T02 refreshes it when the kinds become real.

5. Run the acceptance commands. The crate has only the template's example kind and code at this point; that is expected. It proves the template builds against the real `trigger-core`.

## Acceptance

- [ ] `ls src-tauri/crates/trigger-github/src src-tauri/crates/trigger-github/tests` lists `lib.rs logic.rs os.rs rule.rs` and `live.rs readme.rs`
- [ ] `cd src-tauri && cargo test -p trigger-github` passes: 3 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `grep -n "globset" src-tauri/crates/trigger-github/Cargo.toml` prints one line
- [ ] `grep -n "trigger-github" src-tauri/Cargo.toml` prints nothing
- [ ] `cd src-tauri && cargo clippy -p trigger-github --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-github --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (warnings about template text in the new crate are expected until later cards)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing any real code: kinds, rule fields, logic, OS code. Later cards do that.
- Editing `src-tauri/Cargo.toml` (the workspace already has `globset` and `serde_json`; registering the crate is slice D).
- Editing anything under `plans/_templates/trigger-crate/` unless step 5 fails on template code (see If stuck). The `plans/_templates/` entries in `files:` are only for that case.
- Writing README sections.

## If stuck

- If `src-tauri/crates/trigger-github` already exists, set status blocked and say so. Do not delete or overwrite it.
- If step 2 or 3 says a workspace dependency such as `globset` is not found, the workspace is not as expected. Set status blocked and quote the error.
- If step 5 fails to compile or a template test fails because of a name or signature in the real `trigger-core` (for example `Handle::new`, `RuleError`, `Event` fields), the template is wrong. Make the smallest fix in the matching file under `plans/_templates/trigger-crate/`, delete `src-tauri/crates/trigger-github`, run steps 2 and 3 again, and write what you changed in the Log. Fix only what the compiler reports.
- If `cargo fmt -p trigger-github --check` fails right after step 2, run `cd src-tauri && cargo fmt -p trigger-github` once and carry on.
- If `git status --porcelain` shows `src-tauri/Cargo.lock` changed, that is expected: cargo added the new package.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
