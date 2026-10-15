---
id: T21
epic: 002-trigger-internal
title: Publish tg.agent-finished from the agent watcher
status: done
depends_on: [T20]
size: S
model: haiku
files:
  - src-tauri/src/agent/mod.rs
---

## Goal

When the agent watcher reports a finished agent, it also publishes `InternalSignal::AgentFinished` with the pane id, the pane's directory and the agent name. Starts publish nothing. A small pure function decides which transitions name an agent, and is tested.

## Read first

- `plans/002-trigger-internal/epic.md` Decision 12 and `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` implication 7
- `src-tauri/src/agent/mod.rs:23-30` the imports you extend
- `src-tauri/src/agent/mod.rs:270-295` `emit_transition`, the feed point
- `src-tauri/src/triggers/feeds.rs` `publish` (T20)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance (it was 7 when this card was written).

2. All edits are in `src-tauri/src/agent/mod.rs`. Make them in this order.

3. Find the line `use crate::pty::TerminalId;` (there is exactly one) and replace it with these three lines:

```rust
use crate::pty::{path_string, TerminalId};
use crate::triggers::feeds;
use crate::triggers::internal::InternalSignal;
```

4. In `emit_transition`, find the two lines

```rust
fn emit_transition(app: &AppHandle, id: &TerminalId, transition: Transition) {
    let event = match transition {
```

and put this line between them, so the function starts:

```rust
fn emit_transition(app: &AppHandle, id: &TerminalId, transition: Transition) {
    let finished = finished_agent(&transition).map(str::to_string);
    let event = match transition {
```

5. In the same function, find the line `    let _ = app.emit_to("main", EVENT_AGENT, event);` and the `}` that closes the function directly after it. Replace those two lines with the following, which adds the publish and the new function `finished_agent` after it. How it works: this function runs on the agent watcher thread, which ticks every second, so the only lookup it makes is the pane's directory from the session map (`cwd_of`); it never runs git. A pane that has gone leaves `cwd` empty.

```rust
    let _ = app.emit_to("main", EVENT_AGENT, event);

    // This runs on the watcher thread, so only in memory lookups happen here:
    // the pane's directory is a map read, never a git call.
    if let Some(agent) = finished {
        let cwd = app
            .state::<crate::AppState>()
            .pty
            .cwd_of(id)
            .map(|cwd| path_string(&cwd))
            .unwrap_or_default();
        feeds::publish(
            app,
            InternalSignal::AgentFinished {
                pane: id.clone(),
                cwd,
                agent,
            },
        );
    }
}

/// The agent's name when the transition is a finish; a start names nobody.
fn finished_agent(transition: &Transition) -> Option<&str> {
    match transition {
        Transition::Finished { name, .. } => Some(name),
        Transition::Started { .. } => None,
    }
}
```

6. In the `tests` module at the bottom of the file, find the two lines

```rust
    #[test]
    fn matching_looks_at_the_whole_command_line() {
```

and put this test directly above them:

```rust
    #[test]
    fn only_a_finish_names_an_agent_for_the_trigger_feed() {
        let started = Transition::Started {
            name: "claude".to_string(),
        };
        let finished = Transition::Finished {
            name: "claude".to_string(),
            duration_ms: 9000,
        };
        assert_eq!(finished_agent(&started), None);
        assert_eq!(finished_agent(&finished), Some("claude"));
    }

```

7. Run `cd src-tauri && cargo fmt`, then the commands in Acceptance.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid agent::tests` passes and the list includes `only_a_finish_names_an_agent_for_the_trigger_feed ... ok`
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one (the app crate has existing warnings; this card must not add any)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/002-trigger-internal/tasks/`
- [ ] `grep -c "feeds::publish" src-tauri/src/agent/mod.rs` prints `1`

## Out of scope

- A feed for `Started` or for agent waiting. Only a finish is published (Decision 2 left `tg.agent-waiting` out).
- Calling git, or anything slower than a map read, on the watcher thread.
- Changing `step`, `Transition`, the `terminal://agent` event or its payload.
- Fixing existing clippy warnings, renaming anything, or changing a file not listed under `files:`.

## If stuck

- If `use crate::pty::TerminalId;` is not found exactly once, another card changed the imports: set `status: blocked` and quote the `use crate::pty` lines.
- If `emit_transition` no longer matches the lines quoted above, set `status: blocked` and quote the function.
- If `cargo fmt` changes the code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: clippy baseline before edits = 7 warnings
- 2026-10-12 haiku: built, acceptance passed (agent::tests 7 ok, workspace tests ok, fmt --check 0, no-default-features check 0, clippy 7 = baseline, feeds::publish count 1)
- 2026-10-12 checker: pass
