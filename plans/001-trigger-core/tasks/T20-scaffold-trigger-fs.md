---
id: T20
epic: 001-trigger-core
title: Scaffold the trigger-fs crate from the template
status: done
depends_on: [T08]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-fs/Cargo.toml
  - src-tauri/crates/trigger-fs/README.md
  - src-tauri/crates/trigger-fs/examples/readme.rs
  - src-tauri/crates/trigger-fs/src/lib.rs
  - src-tauri/crates/trigger-fs/src/rule.rs
  - src-tauri/crates/trigger-fs/src/logic.rs
  - src-tauri/crates/trigger-fs/src/os.rs
  - src-tauri/crates/trigger-fs/tests/live.rs
  - src-tauri/crates/trigger-fs/tests/readme.rs
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

`src-tauri/crates/trigger-fs` exists, made by the scaffold script from the template, with its three outside dependencies declared, and it builds and tests clean against the real `trigger-core`. Nothing is `fs` specific yet.

## Read first

- `plans/_templates/trigger-crate/README.md` the file the script copies; its Kinds section is why step 4 exists
- `plans/_tools/new-trigger-crate.mjs` what the script creates and prints
- `src-tauri/crates/trigger-core/src/lib.rs` the contract the template is built against
- `plans/001-trigger-core/epic.md` Decisions 18 and 19 (dependencies, template first)

## Steps

1. Check the crate does not exist yet: `ls src-tauri/crates`. It lists `trigger-core` (and maybe `trigger-hub`), not `trigger-fs`. If `trigger-fs` is there, use If stuck.

2. From the repo root run exactly:

```
node plans/_tools/new-trigger-crate.mjs fs "Fires when files change under a watched folder"
```

It prints `Created .../src-tauri/crates/trigger-fs` and the nine files `Cargo.toml`, `README.md`, `examples/readme.rs`, `src/lib.rs`, `src/rule.rs`, `src/logic.rs`, `src/os.rs`, `tests/live.rs` and `tests/readme.rs` now exist under it. Do not edit the generated files by hand except as the next steps say. The crate needs no entry in the workspace: `members = ["crates/*"]` picks it up.

3. Edit `src-tauri/crates/trigger-fs/Cargo.toml`. Directly under the line `serde_json = { workspace = true }` add these two lines:

```toml
globset = { workspace = true }
notify = { workspace = true }
```

and add this at the very end of the file, after a blank line:

```toml

[dev-dependencies]
uuid = { workspace = true }
```

4. Refresh the Kinds table in the README, because the generated README has instruction text where the table goes. Run `cd src-tauri && cargo run -p trigger-fs --example readme`. It prints a table with a header row, a separator row and one row for `fs.example`. In `src-tauri/crates/trigger-fs/README.md` replace everything between the line `<!-- kinds -->` and the line `<!-- /kinds -->` (keep both marker lines) with that output. The README keeps all its other text for now: card T29 writes it.

5. Run the acceptance commands. The crate has only the template's example kind and code at this point; that is expected. It proves the template builds against the real `trigger-core`.

## Acceptance

- [ ] `ls src-tauri/crates/trigger-fs/src src-tauri/crates/trigger-fs/tests` lists `lib.rs logic.rs os.rs rule.rs` and `live.rs readme.rs`
- [ ] `cd src-tauri && cargo test -p trigger-fs` passes: 3 tests in the library, 1 in `readme`, and `live` shows 1 ignored
- [ ] `cd src-tauri && cargo test -p trigger-core -p trigger-fs` passes
- [ ] `grep -n "globset\|notify\|uuid" src-tauri/crates/trigger-fs/Cargo.toml` prints three lines
- [ ] `cd src-tauri && cargo clippy -p trigger-fs --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-fs --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- Writing any real code: kinds, rule fields, logic, OS code. Later cards do that.
- Editing `src-tauri/Cargo.toml` (the workspace already has `notify`, `globset` and `uuid` from T01).
- Writing the README sections. Only the Kinds table is refreshed here.
- Editing anything under `plans/_templates/trigger-crate/` unless step 5 fails on template code (see If stuck). The `plans/_templates/` entries in `files:` are only for that case.

## If stuck

- If `src-tauri/crates/trigger-fs` already exists, set status blocked and say so. Do not delete or overwrite it.
- If step 2 or 3 says a workspace dependency such as `notify` is not found, T01 did not land. Set status blocked and quote the error.
- If step 5 fails to compile or a template test fails because of a name or signature in the real `trigger-core` (for example `Handle::new`, `RuleError`, `Event` fields), the template is wrong (Decision 19). Make the smallest fix in the matching file under `plans/_templates/trigger-crate/`, delete `src-tauri/crates/trigger-fs`, run steps 2 to 4 again, and write what you changed in the Log. Fix only what the compiler reports.
- If `cargo fmt -p trigger-fs --check` fails right after step 2, run `cd src-tauri && cargo fmt -p trigger-fs` once and carry on.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (scaffolded trigger-fs, added globset/notify and uuid dev-dep, refreshed Kinds table; check-crates exit 0 with expected template-text and trigger-hub size warnings; no template edits needed)
- 2026-10-12 checker: pass
