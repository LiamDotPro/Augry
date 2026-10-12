---
id: R01
epic: 001-trigger-core
question: With `src-tauri/Cargo.toml` as both a package and a workspace root, do `npm run tauri dev` and `tauri build` work unchanged, and does `default-members = [".", "crates/*"]` make a bare `cargo test` cover every crate?
status: answered
---

## Answer

- Yes to both, with one caveat. Adding `[workspace] members = ["crates/*"]` to `src-tauri/Cargo.toml` needs no change to `tauri.conf.json`, the npm scripts or `build.rs`. `tauri build` ran fine.
- Without `default-members`, a bare `cargo test` in `src-tauri` runs ONLY the app package. Verified: dummy crates did not run.
- With `default-members = [".", "crates/*"]`, a bare `cargo test` runs the app and every crate. `cargo test --workspace` works either way and is the safer command.
- `src-tauri/target` and `src-tauri/Cargo.lock` stay the shared ones. The `tauri dev` watcher already watches all of `src-tauri/`, so edits under `crates/` trigger a rebuild.

## Evidence

Run on 2026-10-12 in a scratch copy (rsync of `src-tauri` and `package.json`, target dir cloned), with `crates/dummy-a` and `crates/dummy-b` (one test each), cargo 1.99.0, tauri-cli 2.11.4, tauri-build 2.6.3. Workspace block inserted before `[lib]` in the existing `Cargo.toml`.

- `cargo metadata`: `workspace_members` lists both dummies and `terminal-grid`; `target_directory` is `<src-tauri>/target`.
- With `default-members = [".", "crates/*"]`, bare `cargo test` printed `works_a`, `works_b`, the 76 existing lib tests, and doc tests for all three.
- With `default-members` removed, bare `cargo test` ran only `terminal_grid_lib` and `terminal_grid` (no dummies). `cargo test --workspace` ran all.
- `tauri build --debug --no-bundle -c '{"build":{"beforeBuildCommand":""}}'` (run from `src-tauri`) finished and produced `target/debug/terminal-grid`. Only the frontend build step was skipped, which the workspace cannot affect. I did not run `tauri dev` (needs a GUI).
- Watcher, from tauri-cli source `crates/tauri-cli/src/interface/rust.rs` (read 2026-10-12): `get_watch_folders` always watches the tauri dir (`src-tauri`) recursively, adds in-workspace path dependencies of the app package, plus `build.additionalWatchFolders`. Ignores follow `.taurignore` and built-in rules. So `crates/` is covered both because it sits under `src-tauri` and, once the app depends on them, as path dependencies.
- Existing CI (`.github/workflows/ci.yml:50-52`) runs bare `cargo test` in `src-tauri`; `rust-cache` uses `workspaces: src-tauri`, which still fits.

## Implications for the cards

- Cargo.toml block, placed above `[lib]`:
  ```toml
  [workspace]
  members = ["crates/*"]
  default-members = [".", "crates/*"]
  resolver = "2"   # optional: edition 2021 root package already implies it
  ```
  Setting `default-members` is recommended (Decision 1 leaves it to R01): bare `cargo test` then matches `--workspace`. Keep `.` in it or the app drops out.
- Acceptance lines and CI should still say `cargo test --workspace` (run in `src-tauri`), which is correct whatever `default-members` says. Update `ci.yml` line 52 from `cargo test` to `cargo test --workspace`; same for `cargo clippy --workspace` if added.
- Each crate uses `[dependencies] serde = { workspace = true }` against `[workspace.dependencies]` in the root. Root app deps can stay as they are, or switch to `workspace = true`; not required.
- The app depends on crates via `trigger-hub = { path = "crates/trigger-hub" }` etc. in `[dependencies]`.
- Do NOT touch `tauri.conf.json`, `build.rs`, `package.json` scripts, or `.gitignore` (`src-tauri/target/` already ignored). Do not add a per crate `Cargo.lock` or `target`.
- Crates inside `crates/` need `publish = false`; `members = ["crates/*"]` fails if a directory under it has no `Cargo.toml` (including a stray README only folder).
- Order: create crates and workspace block in the same commit, because a glob member with a missing manifest breaks every cargo command.
- Release profile settings (`[profile.release]`) are honoured only from the root manifest, which stays the app. Crates must not define profiles.

## Doubts

- `tauri dev` was not run live; the watcher conclusion comes from reading the CLI source plus the fact that `crates/` is inside the watched `src-tauri` dir.
- Windows and Linux were not run; workspace behaviour is platform independent. `tools/api-codegen` has its own manifest outside `src-tauri` and is unaffected.
