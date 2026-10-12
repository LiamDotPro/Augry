# Conventions

Rules for code and text in this repo. Builders follow them; checkers enforce
them.

## General

- Match the code around you. If a file uses double quotes, so do you.
- No drive by changes: no renames, reformatting or upgrades a card did not ask
  for.
- Comments explain why, not what. Keep the existing tone: short, plain, full
  sentences. Module docs (`//!`) say what the module is for and point at the
  section of `docs/technical-design.md` that designed it.
- Never commit secrets. Nothing a user types, copies or has on disk goes into
  a log line, an event payload or a test fixture.
- This is a public repo. Nothing from the cloud repo comes here except the
  released API contract in `libs/api-contract/`, delivered by its workflow.

## Rust

- Edition 2021, the stable toolchain, `cargo fmt` default settings. `cargo fmt
  --check` must pass.
- New crates under `src-tauri/crates/` are `cargo clippy --all-targets --
  -D warnings` clean. The app crate has a few existing warnings; do not add to
  them and do not fix them in a card that is about something else.
- Dependencies are declared once in `[workspace.dependencies]` of
  `src-tauri/Cargo.toml` and used with `{ workspace = true }`. A new dependency
  is a Decision in the epic, with the version the research note found.
- Errors are `thiserror` enums. Anything that crosses IPC is serialised with
  `#[serde(rename_all = "camelCase")]`, and `AppError` keeps its `code` tag.
- No `unwrap` or `expect` in library code; `?` with a typed error. Tests may
  `unwrap`.
- Platform code is behind `#[cfg(target_os = "macos")]`, `"windows"` or
  `"linux"`, in its own file (`macos.rs`) when it is more than a few lines. The
  crate compiles on every platform; the parts that cannot run return
  `Availability::Unsupported`.
- Threads are named (`std::thread::Builder::new().name(...)`) and stop when
  their handle is dropped. Nothing blocks the Tauri main thread: command
  handlers that do work are `async` and use `crate::blocking`.
- Pure logic first. A watcher is a thin OS layer over a pure `step` or `match`
  function with its own unit tests, the way `agent::step` is written.
- Tests sit beside the code in `#[cfg(test)] mod tests`. Tests that touch the
  real OS go in `tests/live.rs` with `#[ignore]` and a comment saying how to
  run them. `cargo test` must pass offline and without a display.
- Temporary files in tests go under `std::env::temp_dir()` with a uuid, and
  are removed at the end.

## Trigger crates

The rules are in `docs/process-triggers.md` Part 2, "Simple and isolated: the
rules". In short, and checked by `node plans/_tools/check-crates.mjs`:

- A crate is made with `node plans/_tools/new-trigger-crate.mjs <name>
  "<summary>"`, never by hand, and keeps the template's files: `src/lib.rs`
  (glue), `src/rule.rs` and `src/logic.rs` (pure, with the unit tests),
  `src/os.rs` (the only OS code), `tests/readme.rs`, `tests/live.rs`.
- It depends on `trigger-core` and outside crates only. Never another trigger
  crate, never `tauri`, never the app.
- Its README has the eight sections: Kinds, Rule fields, Payload, Platforms,
  Permissions, Example, Develop, Known limits. A card that changes what a crate
  does updates its README in the same card. Refresh the Kinds table with
  `cargo run -p trigger-<name> --example readme`.
- About ten kinds and one OS mechanism family per crate. A file over 400 lines
  is a warning, over 800 an error.
- A bug fix lands in `rule.rs` or `logic.rs` with a unit test whenever it can.

## TypeScript and React

- Strict TypeScript, React 19, no new UI libraries. `npm run build` (which
  type checks) must pass.
- Components never call `call()` from `src/ipc/client.ts`. They call an action
  from `useAppActions()`; the action does the IPC and the error handling.
- A new command is added to `Commands` in `src/ipc/types.ts` and to section 10
  of `docs/technical-design.md` in the same card. Events use the `area://name`
  form and a constant in `EVENTS`.
- New UI state goes in the reducer (`src/state/reducer.ts`) with an `Action`
  type like `triggers/armed`. State that survives a restart goes in
  `toSession` or in `Config`, never in `localStorage`.
- Pure logic goes in `src/lib/` with a vitest file beside it. There are no
  component tests; keep components thin and the logic testable.
- Colours and sizes come from `src/styles/tokens.css`. Class names are
  `area__part` like the files around you. A new screen comes from a Claude
  Design export in `design/`; see `CLAUDE.md`.

## Git

- One commit per card: `<epic> <card id>: <card title>`.
- Branch per epic: `epic/<epic id>`. Parallel epics build in separate
  worktrees.

## Writing (docs, cards, commit messages)

- Plain and direct. No filler, no marketing words.
- No dashes as punctuation. Use a comma, a colon, or a new sentence.
- British spelling (colour, behaviour), which is what the docs use.
