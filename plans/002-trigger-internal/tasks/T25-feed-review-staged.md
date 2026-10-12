---
id: T25
epic: 002-trigger-internal
title: Publish tg.review-staged from review_stage
status: todo
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/git/review.rs
---

## Goal

`review_stage` publishes `InternalSignal::ReviewStaged` after `git add` has succeeded, with the pane's repository root and the number of paths the user staged.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 7
- `src-tauri/src/git/review.rs:12-18` the imports you extend
- `src-tauri/src/git/review.rs:572-582` `review_stage`, the feed point
- `src-tauri/src/triggers/feeds.rs` `publish`, `repo_root_of`, `review_staged` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/git/review.rs`. Make them in this order.

3. Find the line `use crate::pty::TerminalId;` (there is exactly one) and replace it with:

```rust
use crate::pty::TerminalId;
use crate::triggers::feeds;
```

4. In `review_stage`, find this closure body:

```rust
    crate::blocking(move || {
        stage(&cwd_of(&app, &id)?, &paths)?;
        refresh_labels(&app, &id);
        Ok(())
    })
```

and replace it with. `refresh_labels` has just read the repository, so `repo_root_of` is served from the git cache:

```rust
    crate::blocking(move || {
        let cwd = cwd_of(&app, &id)?;
        stage(&cwd, &paths)?;
        refresh_labels(&app, &id);
        let state = app.state::<crate::AppState>();
        if let Some(repo) = feeds::repo_root_of(&state, &cwd) {
            feeds::publish(&app, feeds::review_staged(&repo, paths.len()));
        }
        Ok(())
    })
```

5. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid git::review` passes (the existing tests, including `stage_edit_and_unstage_round_trip`; the counting decision is `feeds::review_staged`, tested by T20)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/git/review.rs` prints `1`

## Out of scope

- Publishing from `review_unstage` or `review_write_file`.
- Changing `stage`, `unstage`, `repo_root` or any payload of the review commands.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the quoted closure body is not found in `review_stage`, set `status: blocked` and quote the function.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
