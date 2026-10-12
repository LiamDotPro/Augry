---
id: T26
epic: 002-trigger-internal
title: Publish tg.note-saved from notes_write
status: todo
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/notes/mod.rs
---

## Goal

`notes_write` publishes `InternalSignal::NoteSaved` after the write has succeeded, with the file's path and its path relative to the notes root (forward slashes, no leading slash).

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 11 and 12
- `src-tauri/src/notes/mod.rs:460-473` `notes_write`, the feed point
- `src-tauri/src/triggers/feeds.rs` `publish` and `note_saved` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. The only edit is in `src-tauri/src/notes/mod.rs`, in `notes_write`. Find this closure body:

```rust
    crate::blocking(move || {
        app.state::<crate::AppState>()
            .notes
            .write(&rel_path, &content, expected_mtime_ms)
    })
```

and replace it with. How it works: the `?` after `write` returns early on a conflict or an error, so the signal goes out only for a write that succeeded.

```rust
    crate::blocking(move || {
        let state = app.state::<crate::AppState>();
        let result = state.notes.write(&rel_path, &content, expected_mtime_ms)?;
        let signal = crate::triggers::feeds::note_saved(&state.notes.root(), &rel_path);
        crate::triggers::feeds::publish(&app, signal);
        Ok(result)
    })
```

3. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid notes::` passes (the existing tests; the path decision is `feeds::note_saved`, tested by T20)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/notes/mod.rs` prints `1`

## Out of scope

- Publishing from `notes_create`, `notes_rename`, `notes_delete` or the file watcher for outside changes. Only saves through `notes_write` count.
- Changing `NotesService::write` or its error handling.
- Note tags (`tg.note-tagged` is a non goal of the epic).
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the quoted closure body is not found, set `status: blocked` and quote `notes_write`.
- If the compiler cannot infer the closure's error type, the `?` and `Ok(result)` lines were changed: restore them as given.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
