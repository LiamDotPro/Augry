---
id: T01
epic: 001-trigger-core
title: Add the Cargo workspace and the trigger-core skeleton
status: todo
depends_on: []
size: S
model: haiku
files:
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/crates/trigger-core/Cargo.toml
  - src-tauri/crates/trigger-core/src/lib.rs
  - src-tauri/crates/trigger-core/README.md
---

## Goal

`src-tauri` becomes a Cargo workspace and an empty but valid `trigger-core` crate exists in it. Both land in this one card because a glob member (`crates/*`) with no manifest breaks every cargo command.

## Read first

- `plans/_context/conventions.md` (sections "Rust" and "Trigger crates")
- `src-tauri/Cargo.toml` the file you edit; the new block goes above `[lib]`
- `plans/001-trigger-core/research/R01-workspace.md` why `default-members` is set

## Steps

1. Edit `src-tauri/Cargo.toml`. Insert this block between the `[package]` table (which ends at the `edition = "2021"` and `description` lines) and the `[lib]` table, with one blank line before and after it. Change nothing else in the file. Do NOT add a `[features]` table or any `trigger-*` dependency.

```toml
[workspace]
members = ["crates/*"]
default-members = [".", "crates/*"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
notify = "8.2"
uuid = { version = "1", features = ["v4"] }
globset = "0.4.20"
```

2. Create `src-tauri/crates/trigger-core/Cargo.toml` with exactly:

```toml
[package]
name = "trigger-core"
version = "0.1.0"
edition = "2021"
publish = false
description = "The contract between trigger sources and whatever runs them, plus a test kit"

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

3. Create `src-tauri/crates/trigger-core/src/lib.rs` with exactly:

```rust
//! The contract between trigger sources and whatever runs them. A source
//! depends on this crate only. See docs/process-triggers.md Part 2.
```

4. Create `src-tauri/crates/trigger-core/README.md` with exactly this text (the check script needs the `## Develop` heading):

```markdown
# trigger-core

The contract between trigger sources and whatever runs them (types, the
`Source` trait) and a small test kit. Design: `docs/process-triggers.md`
Part 2.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-core`
- `cargo clippy -p trigger-core --all-targets -- -D warnings`
- `cargo fmt -p trigger-core --check`

From the repo root: `node plans/_tools/check-crates.mjs`.
```

5. Run `cd src-tauri && cargo test -p trigger-core`. It builds and runs 0 tests. This updates `src-tauri/Cargo.lock` (a new `trigger-core` entry); that is expected.

## Acceptance

- [ ] `cd src-tauri && cargo metadata --no-deps --format-version 1 > /dev/null` exits 0
- [ ] `cd src-tauri && cargo test -p trigger-core` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid` exits 0 (the app still builds)
- [ ] `cd src-tauri && cargo fmt -p trigger-core --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `git diff src-tauri/Cargo.toml` shows only added lines, all in the block from step 1
- [ ] `git status --porcelain` lists only paths under `files:` (the untracked `crates/` folder counts as `src-tauri/crates/trigger-core/`)

## Out of scope

- The `[features]` table and the app's path dependencies on the crates. That is slice D.
- Any type, trait or test in `lib.rs`. Later cards add them.
- Using `new-trigger-crate.mjs`. It makes source crates; `trigger-core` is written by hand.
- Switching the app's own dependencies to `workspace = true`.
- Touching `tauri.conf.json`, `build.rs`, `package.json` or `.gitignore`.

## If stuck

- If cargo says `failed to load manifest for workspace member`, the `crates/trigger-core/Cargo.toml` path or name is wrong; compare with step 2.
- If `cargo check -p terminal-grid` fails on code you did not touch, do not try to fix it and do not use `git stash`. Set status blocked and quote the first error in the Log.
- If cargo wants network access and has none, set status blocked.
- Anything else: set status blocked and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
