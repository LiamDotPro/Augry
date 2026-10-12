---
id: T27
epic: 002-trigger-internal
title: Publish tg.focus-entered from set_focused_terminal
status: todo
depends_on: [T23]
size: S
model: haiku
files:
  - src-tauri/src/pty/mod.rs
---

## Goal

`set_focused_terminal` publishes `InternalSignal::FocusEntered` with the pane id and its directory when focus moves onto a pane that did not have it. Setting focus to the pane that already has it, or to none, publishes nothing.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 9
- `src-tauri/src/pty/mod.rs` the function `set_focused_terminal` (search for `pub fn set_focused_terminal`)
- `src-tauri/src/pty/mod.rs` `PtyManager::focused` and `PtyManager::cwd_of`
- `src-tauri/src/triggers/feeds.rs` `publish` and `focus_entered` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. The only edit is in `src-tauri/src/pty/mod.rs`, in `set_focused_terminal`. Do not use line numbers: another slice edits other parts of this file, so find the text below. In `set_focused_terminal`, find these four consecutive lines:

```rust
    state.pty.set_focused(id.clone());
    let Some(id) = id else {
        return Ok(());
    };
```

and replace them with the following. How it works: `previous` is read before the new id is stored, so `focus_entered` can tell a move from a repeat. `cwd_of` is a lookup in the session map, so it is safe on the thread this command runs on. An id with no session publishes nothing.

```rust
    let previous = state.pty.focused();
    state.pty.set_focused(id.clone());
    let Some(id) = id else {
        return Ok(());
    };

    if let Some(cwd) = state.pty.cwd_of(&id) {
        if let Some(signal) =
            crate::triggers::feeds::focus_entered(previous.as_deref(), &id, &path_string(&cwd))
        {
            crate::triggers::feeds::publish(&app, signal);
        }
    }
```

Leave everything after those lines (the `app_handle` clone and the `spawn_blocking` that refreshes git) as it is.

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid triggers::feeds` passes (the `focus_fires_only_when_it_moves_to_another_pane` test from T20 covers the decision; this card adds none)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/pty/mod.rs` prints `2` (T23 added the first)
- [ ] `grep -c "feeds::focus_entered" src-tauri/src/pty/mod.rs` prints `1`

## Out of scope

- The reader thread, `write_terminal`, `shell.rs` and anything else in `pty/`: another slice edits them.
- Changing `set_focused`, `PtyManager`, the git refresh after the focus change, or the frontend's calls to this command.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the four quoted lines are not found together in `set_focused_terminal`, set `status: blocked` and quote the function.
- If `path_string` is not in scope, it is defined in this same file as `pub fn path_string`; if it is gone, set `status: blocked`.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
