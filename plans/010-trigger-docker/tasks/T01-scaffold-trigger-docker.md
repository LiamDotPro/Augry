---
id: T01
epic: 010-trigger-docker
title: Scaffold the trigger-docker crate from the template
status: todo
depends_on: []
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/Cargo.toml
  - src-tauri/crates/trigger-docker/README.md
  - src-tauri/crates/trigger-docker/examples/readme.rs
  - src-tauri/crates/trigger-docker/src/lib.rs
  - src-tauri/crates/trigger-docker/src/rule.rs
  - src-tauri/crates/trigger-docker/src/logic.rs
  - src-tauri/crates/trigger-docker/src/os.rs
  - src-tauri/crates/trigger-docker/tests/live.rs
  - src-tauri/crates/trigger-docker/tests/readme.rs
  - src-tauri/Cargo.lock
---

## Goal

`src-tauri/crates/trigger-docker` exists, made by the scaffold script from the template, with its one outside dependency (`globset`) declared. It builds and tests clean against the real `trigger-core`. Nothing is docker specific yet.

## Read first

- `plans/_templates/trigger-crate/README.md` the file the script copies
- `plans/_tools/new-trigger-crate.mjs` what the script creates and prints
- `plans/010-trigger-docker/epic.md` Decisions 1 to 3 (crate, kinds, `when`)

## Steps

1. Check the crate does not exist yet: `ls src-tauri/crates`. It lists `trigger-core`, `trigger-fs` and `trigger-hub`, not `trigger-docker`. If `trigger-docker` is there, use If stuck.

2. From the repo root run exactly:

```
node plans/_tools/new-trigger-crate.mjs docker "Fires on container and image events from Docker and Podman"
```

It prints `Created .../src-tauri/crates/trigger-docker` and the nine files `Cargo.toml`, `README.md`, `examples/readme.rs`, `src/lib.rs`, `src/rule.rs`, `src/logic.rs`, `src/os.rs`, `tests/live.rs` and `tests/readme.rs` now exist under it. Do not edit the generated files by hand except as the next step says. The crate needs no entry in the workspace: `members = ["crates/*"]` picks it up.

3. Edit `src-tauri/crates/trigger-docker/Cargo.toml`. Directly under the line `serde_json = { workspace = true }` add this one line:

```toml
globset = { workspace = true }
```

Add nothing else. The crate has no dev-dependencies.

4. Run the acceptance commands. The crate has only the template's example kind and code at this point; that is expected. The generated README Kinds table already matches the template's example kind, so the README test passes without edits.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 3 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings: the README still has template text, and `src/logic.rs` and `src/os.rs` still have template code; those are expected until slice C)
- [ ] `ls src-tauri/crates/trigger-docker/src src-tauri/crates/trigger-docker/tests` lists `lib.rs logic.rs os.rs rule.rs` and `live.rs readme.rs`
- [ ] `grep -c globset src-tauri/crates/trigger-docker/Cargo.toml` prints `1`
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing any real code: kinds, rule fields, logic, OS code. Later cards do that.
- Editing `src-tauri/Cargo.toml`. The crate is picked up by the workspace glob, and registering it in the app is slice D.
- Adding any dependency other than `globset`. `serde` and `serde_json` are already there.
- Editing anything under `plans/_templates/`.
- Editing the README.

## If stuck

- If `src-tauri/crates/trigger-docker` already exists, set status blocked and say so. Do not delete or overwrite it.
- If `node` or `rustfmt` is missing, set status blocked and say which.
- If the template fails to compile against `trigger-core` (a name or signature error), set status blocked and quote the first compiler error. Do not edit the template.
- If `cargo fmt -p trigger-docker --check` fails right after step 2, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.

- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
