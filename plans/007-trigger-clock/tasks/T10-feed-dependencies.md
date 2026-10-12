---
id: T10
epic: 007-trigger-clock
title: Add the calendar and HTTP dependencies
status: todo
depends_on: [T09]
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/crates/trigger-clock/Cargo.toml
---

## Goal

`trigger-clock` depends on `calcard`, `ureq`, `iana-time-zone` and (at run time now, not only in tests) `chrono-tz`, at the versions research found. Nothing uses them yet; the next cards do.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 6 (the dependency list and versions)
- `plans/007-trigger-clock/research/R02-ics-and-http.md` "Implications for the cards" (why `default-features = false` for `calcard`, why `chrono-tz` stays on 0.10)
- `src-tauri/Cargo.toml:9-20` the `[workspace.dependencies]` block you add to
- `src-tauri/crates/trigger-clock/Cargo.toml` the file you edit

## Steps

1. Edit `src-tauri/Cargo.toml`. In `[workspace.dependencies]`, directly under the line `chrono-tz = "0.10"` (added by T01), add these three lines:

```toml
calcard = { version = "0.3.14", default-features = false }
ureq = "3.4"
iana-time-zone = "0.1.65"
```

`0.1.65` is the version already in `src-tauri/Cargo.lock` (chrono pulls it in), so the lock does not get a second copy.

2. Edit `src-tauri/crates/trigger-clock/Cargo.toml`. Directly under the line `chrono = { workspace = true }` add:

```toml
chrono-tz = { workspace = true }
calcard = { workspace = true }
ureq = { workspace = true }
iana-time-zone = { workspace = true }
```

3. In the same file, delete the `[dev-dependencies]` section at the end: the blank line before it, the line `[dev-dependencies]` and the line `chrono-tz = { workspace = true }`. `chrono-tz` is now a normal dependency, and the feed code needs it outside tests. The file now ends with the comment lines about platform only dependencies.

4. Run `cd src-tauri && cargo build -p trigger-clock`. It downloads and compiles the new crates. `Cargo.lock` gains `calcard`, `ureq` and their dependencies; that is expected. Do not run `cargo update`.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all slice A tests still pass; `live` shows 1 ignored)
- [ ] `grep -n "^calcard\|^ureq\|^iana-time-zone" src-tauri/Cargo.toml` prints 3 lines
- [ ] `grep -c "workspace = true" src-tauri/crates/trigger-clock/Cargo.toml` prints `9`
- [ ] `grep -c "dev-dependencies" src-tauri/crates/trigger-clock/Cargo.toml` prints `0`
- [ ] `cd src-tauri && cargo tree -p trigger-clock -e normal --depth 1` lists `calcard v0.3.14`, `ureq v3.4`, `iana-time-zone v0.1.65` and `chrono-tz v0.10`
- [ ] `cd src-tauri && cargo tree -p trigger-clock -e normal | grep -c tokio` prints `0` (the crate must never pull in tokio)
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (the `still has template code` warning for `src/lib.rs` is expected until slice C)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Using any of the new crates in code.
- Adding `reqwest` or `tokio`, or the `jmap` feature of `calcard`, or any ureq feature other than the defaults.
- Upgrading any other entry in `Cargo.lock`.
- Registering the crate in the app.

## If stuck

- If `cargo build` says a workspace dependency is not found, a line from step 1 is missing or misspelt. Fix the line; do not write a version into the crate's own `Cargo.toml`.
- If `cargo tree` shows `iana-time-zone` twice, or `grep -c tokio` prints more than `0`, set `status: blocked` and paste the `cargo tree` lines.
- If cargo cannot download a crate (network), set `status: blocked` and quote the error.
- If the crate says `chrono-tz` is declared twice, step 3 was missed. Do it.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
