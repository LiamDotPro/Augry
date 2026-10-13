---
id: T23
epic: 002-trigger-internal
title: Publish tg.pane-opened-in-repo from create_terminal
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/pty/mod.rs
---

## Goal

After a pane is spawned, `create_terminal` publishes `InternalSignal::PaneOpened` with the pane id, its directory and the repository root (None outside a repository, which fires nothing). The repository lookup runs inside the command's existing `blocking` closure and only while process triggers are switched on.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 7
- `src-tauri/src/pty/mod.rs` the function `create_terminal` (search for `pub async fn create_terminal`)
- `src-tauri/src/triggers/feeds.rs` `publish`, `enabled`, `repo_root_of` (T20)
- `src-tauri/src/triggers/mod.rs` `refresh_now` and `refresh_app`

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. The only edit is in `src-tauri/src/pty/mod.rs`, in `create_terminal`. Do not use line numbers: another slice edits other parts of this file, so find the text below. In `create_terminal`, find these three consecutive lines:

```rust
        state.agent.wake();
        crate::triggers::refresh_app(&app);
        Ok(info)
```

(`restart_terminal` has `state.agent.wake();` followed directly by `Ok(info)`; it is not the one to change, and `close_terminal` and `report_cwd` also call `refresh_app` but not after `agent.wake()`.) Replace those three lines with the following. How it works: the rules that name a repository with `${repo.root}` only exist after a refresh that has seen the new pane, so the refresh must finish before the signal goes out. `refresh_app` runs on a spawned thread, so it is replaced by `refresh_now`, which runs here, and this closure is already on a blocking thread. `refresh_now` fills the git cache for the new pane, so `repo_root_of` reads the cache.

```rust
        state.agent.wake();
        // The rules that name this pane's repository (`${repo.root}`) must be
        // armed before the signal goes out, so refresh here instead of on a
        // spawned thread. This closure already runs on a blocking thread.
        crate::triggers::refresh_now(&state);
        if crate::triggers::feeds::enabled(&state) {
            let repo_root = crate::triggers::feeds::repo_root_of(&state, Path::new(&info.cwd));
            crate::triggers::feeds::publish(
                &app,
                crate::triggers::internal::InternalSignal::PaneOpened {
                    pane: info.id.clone(),
                    cwd: info.cwd.clone(),
                    repo_root,
                },
            );
        }
        Ok(info)
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::` passes (this card adds no test: the glue needs a running window; T29 tests the signal end to end)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/pty/mod.rs` prints `1`
- [ ] `grep -c "refresh_app" src-tauri/src/pty/mod.rs` prints `2` (still used by `close_terminal` and `report_cwd`)

## Out of scope

- The reader thread, `write_terminal`, `shell.rs` and anything else in `pty/`: another slice edits them.
- Publishing on `restart_terminal`, or when a pane changes directory into a repository.
- Changing `spawn`, `TerminalInfo` or the command's signature.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the three quoted lines are not found together in `create_terminal`, set `status: blocked` and quote the end of that function.
- If another change has already replaced `refresh_app` in `create_terminal`, set `status: blocked`; do not add a second refresh.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed. Clippy baseline 7, after edit 7. `cargo test -p terminal-grid pty::` matched 0 tests (passes, nothing run); `cargo test --workspace` all green.
- 2026-10-12 architect: moved the refresh and the publish onto one spawned blocking thread, so a new pane never waits for sources to start (git snapshots, docker's version check, gh's sign in check can take seconds). Order is kept: refresh, then publish. Tests 205 pass, clippy 7.
- 2026-10-12 checker: pass
