---
id: T22
epic: 002-trigger-internal
title: Publish tg.task-reported from the task tracker
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/agent/task.rs
---

## Goal

When an agent writes a new task line, `TaskTracker::poll` also publishes `InternalSignal::TaskReported` with the pane id and the pane's directory. Emptying the file (clearing the task) publishes nothing.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 7
- `src-tauri/src/agent/task.rs:12-20` the imports you extend
- `src-tauri/src/agent/task.rs:115-150` `TaskTracker::poll`, the feed point
- `src-tauri/src/triggers/feeds.rs` `publish` and `task_reported` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/agent/task.rs`. Make them in this order.

3. Find the line `use tauri::{AppHandle, Emitter};` and replace it with:

```rust
use tauri::{AppHandle, Emitter, Manager};
```

4. Find the line `use crate::pty::TerminalId;` and replace it with:

```rust
use crate::pty::{path_string, TerminalId};
use crate::triggers::feeds;
```

5. In `TaskTracker::poll`, find this statement (the last one in the `for` loop body):

```rust
            let _ = app.emit_to(
                "main",
                EVENT_TASK,
                TaskEvent {
                    id: id.clone(),
                    task,
                    updated_at_ms,
                },
            );
```

and replace it with the following. How it works: `reported` is a copy of the task kept before the event takes it; `feeds::task_reported` returns None when the task is empty, so a cleared task publishes nothing. `poll` runs on the agent watcher thread, so the only lookup is the pane's directory from the session map.

```rust
            let reported = task.clone();
            let _ = app.emit_to(
                "main",
                EVENT_TASK,
                TaskEvent {
                    id: id.clone(),
                    task,
                    updated_at_ms,
                },
            );

            // Only a map read, because this is the agent watcher thread.
            let cwd = app
                .state::<crate::AppState>()
                .pty
                .cwd_of(id)
                .map(|cwd| path_string(&cwd))
                .unwrap_or_default();
            if let Some(signal) = feeds::task_reported(id, &cwd, reported.as_deref()) {
                feeds::publish(app, signal);
            }
```

6. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid agent::task` passes (the six existing tests; this card adds none, the decision it makes is `feeds::task_reported`, tested by T20)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::task_reported" src-tauri/src/agent/task.rs` prints `1`

## Out of scope

- Changing `parse_task`, the stamp logic, the `terminal://task` event or its payload.
- Putting the task text in the signal. The payload is the pane and its directory only (Decision 3).
- Calling git on the watcher thread.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If the `emit_to` statement does not match the lines quoted above, set `status: blocked` and quote the loop body of `poll`.
- If `cargo check` says `task` was moved, the `let reported = task.clone();` line was put after the `emit_to` call: move it above.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed (clippy baseline 7, after 7; agent::task 6 passed; workspace tests ok; fmt ok; no-default-features check ok)
- 2026-10-12 checker: pass
