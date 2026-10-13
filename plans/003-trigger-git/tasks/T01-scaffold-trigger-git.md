---
id: T01
epic: 003-trigger-git
title: Scaffold the trigger-git crate from the template
status: done
depends_on: []
size: S
model: haiku
files:
  - src-tauri/crates/trigger-git/Cargo.toml
  - src-tauri/crates/trigger-git/README.md
  - src-tauri/crates/trigger-git/examples/readme.rs
  - src-tauri/crates/trigger-git/src/lib.rs
  - src-tauri/crates/trigger-git/src/rule.rs
  - src-tauri/crates/trigger-git/src/logic.rs
  - src-tauri/crates/trigger-git/src/os.rs
  - src-tauri/crates/trigger-git/tests/live.rs
  - src-tauri/crates/trigger-git/tests/readme.rs
  - src-tauri/Cargo.lock
---

## Goal

`src-tauri/crates/trigger-git` exists, made by the scaffold script from the template, with its outside dependencies declared, and it builds and tests clean against the real `trigger-core`. Nothing is git specific yet.

## Read first

- `plans/_templates/trigger-crate/README.md` the file the script copies
- `plans/_tools/new-trigger-crate.mjs` what the script creates and prints
- `src-tauri/crates/trigger-fs/Cargo.toml` the finished example of a trigger crate manifest
- `plans/003-trigger-git/epic.md` Decision 1 (the crate and its dependencies)

## Steps

1. Check the crate does not exist yet: `ls src-tauri/crates`. It lists `trigger-core`, `trigger-fs` and `trigger-hub`, not `trigger-git`. If `trigger-git` is there, use If stuck.

2. From the repo root run exactly:

```
node plans/_tools/new-trigger-crate.mjs git "Fires when something happens in a git repository"
```

It prints `Created .../src-tauri/crates/trigger-git` and the nine files `Cargo.toml`, `README.md`, `examples/readme.rs`, `src/lib.rs`, `src/rule.rs`, `src/logic.rs`, `src/os.rs`, `tests/live.rs` and `tests/readme.rs` now exist under it. Do not edit the generated files by hand except as the next step says. The crate needs no entry in the workspace: `members = ["crates/*"]` picks it up.

3. Edit `src-tauri/crates/trigger-git/Cargo.toml`. Directly under the line `serde_json = { workspace = true }` add these two lines:

```toml
globset = { workspace = true }
notify = { workspace = true }
```

and add this at the very end of the file, after a blank line:

```toml

[dev-dependencies]
uuid = { workspace = true }
```

`globset` is for the branch glob (T03), `notify` for the watcher (slice C), `uuid` for the temp repos in the live tests (slice C). The workspace already declares all three.

4. The generated README already has a valid example Kinds table, so `tests/readme.rs` passes as it is. Do not touch the README.

5. Run the acceptance commands. The crate has only the template's example kind and code at this point; that is expected.

## Acceptance

- [ ] `ls src-tauri/crates/trigger-git/src src-tauri/crates/trigger-git/tests` lists `lib.rs logic.rs os.rs rule.rs` and `live.rs readme.rs`
- [ ] `cd src-tauri && cargo test -p trigger-git` passes: 3 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `grep -c "globset\|notify\|uuid" src-tauri/crates/trigger-git/Cargo.toml` prints `3`
- [ ] `cd src-tauri && cargo clippy -p trigger-git --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-git --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it prints a `README.md still has template text` warning and warnings about template code; that is expected until later cards)
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing any real code: kinds, rule fields, logic, OS code. Later cards do that.
- Editing `src-tauri/Cargo.toml`. The workspace already has `notify`, `globset` and `uuid`. Registering the crate in the app is slice D.
- Writing README sections or refreshing the Kinds table (T02 and T09).
- Editing anything under `plans/_templates/`.

## If stuck

- If `src-tauri/crates/trigger-git` already exists, set status blocked and say so. Do not delete or overwrite it.
- If step 3 or the build says a workspace dependency such as `notify` or `globset` is not found, set status blocked and quote the error.
- If `cargo fmt -p trigger-git --check` fails right after step 2, run `cd src-tauri && cargo fmt -p trigger-git` once and carry on.
- If a template test fails to compile against `trigger-core`, set status blocked and quote the error. Do not edit the template.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
