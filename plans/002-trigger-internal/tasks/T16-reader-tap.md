---
id: T16
epic: 002-trigger-internal
title: Feed command, bell and output signals from the reader thread
status: done
depends_on: [T15, T08]
size: S
model: sonnet
files:
  - src-tauri/src/pty/mod.rs
---

## Goal

Every terminal now publishes trigger signals: when a command ends (with its exit code and how long it ran), when the bell rings, and, only while a `tg.output-matched` rule is armed, for each output line. The reader thread gives each chunk to a `tap::Tap` right after queuing it for the window, and `PtyManager::write` tells the session's `CommandClock` when Enter is pressed. The card is `model: sonnet` because it edits the reader thread and the sessions lock path: a mistake there can hang every terminal, and the compiler and unit tests cannot see it, so the builder must follow the lock rule in step 5 and re-read its own edit.

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 8, 9, 10 and 12
- `plans/002-trigger-internal/research/R01-feeds-and-shell-integration.md` "Output chunks in the reader thread"
- `src-tauri/src/pty/mod.rs:61-75` `Session`
- `src-tauri/src/pty/mod.rs:187-330` `spawn` and `start_reader`
- `src-tauri/src/pty/mod.rs:355-370` `write`
- `src-tauri/src/pty/tap.rs` `Tap::process`, `CommandClock` (T15)

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. All edits are in `src-tauri/src/pty/mod.rs`. Use exact find and replace, in this order.

3. Add the clock to `Session`. Find:

```rust
    /// Bumped on restart so the previous reader and waiter threads fall silent.
    generation: u64,
}
```

Replace with:

```rust
    /// Bumped on restart so the previous reader and waiter threads fall silent.
    generation: u64,
    /// Times commands for the internal triggers. `write` starts it, the reader
    /// thread stops it.
    clock: Arc<Mutex<tap::CommandClock>>,
}
```

4. In `PtyManager::spawn`, make three edits.

4a. Find:

```rust
        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
```

Replace with:

```rust
        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        let clock = Arc::new(Mutex::new(tap::CommandClock::default()));
```

4b. Find:

```rust
            spawned_at: Instant::now(),
            generation,
        };
```

Replace with:

```rust
            spawned_at: Instant::now(),
            generation,
            clock: Arc::clone(&clock),
        };
```

4c. Find:

```rust
        self.start_reader(app.clone(), id.clone(), generation, output);
```

Replace with:

```rust
        self.start_reader(app.clone(), id.clone(), generation, output, clock);
```

5. In `PtyManager::write`, find:

```rust
        session.writer.write_all(bytes)?;
```

Replace with the following. The clock is locked while the sessions lock is held, so the reader thread must never lock the clock and then ask for the sessions lock; `Tap::process` was written to respect that. Do not move this block, and do not add any other call that takes a lock here.

```rust
        if bytes.contains(&b'\r') {
            session
                .clock
                .lock()
                .expect("pty clock")
                .on_enter(Instant::now());
        }
        session.writer.write_all(bytes)?;
```

6. In `start_reader`, make three edits.

6a. Find:

```rust
        generation: u64,
        mut output: Box<dyn Read + Send>,
    ) {
        let pump = Arc::new(OutputPump::new());
        let sessions = Arc::clone(&self.sessions);
```

Replace with:

```rust
        generation: u64,
        mut output: Box<dyn Read + Send>,
        clock: Arc<Mutex<tap::CommandClock>>,
    ) {
        let pump = Arc::new(OutputPump::new());
        let sessions = Arc::clone(&self.sessions);
        // The reader thread publishes trigger signals, so it needs the app.
        let tap_app = app.clone();
        let mut tap = tap::Tap::new(id.clone(), clock);
```

6b. Find:

```rust
                            pump.push(&buffer[..count]);
```

Replace with (the publish call goes after the push, so the screen is never delayed):

```rust
                            pump.push(&buffer[..count]);
                            publish_signals(&tap_app, &mut tap, &buffer[..count]);
```

6c. Directly above the line `fn emit_output(app: &AppHandle, id: &TerminalId, bytes: Vec<u8>) -> bool {` insert this function:

```rust
/// Gives a chunk of output to the session's tap and publishes what it finds
/// (command ended, bell, output lines) to the internal triggers. Runs after the
/// chunk is queued for the window, so it never delays the screen.
fn publish_signals(app: &AppHandle, tap: &mut tap::Tap, chunk: &[u8]) {
    let state = app.state::<crate::AppState>();
    let signals = tap.process(
        chunk,
        state.triggers.wants_output(),
        Instant::now(),
        |pane| {
            state
                .pty
                .cwd_of(&pane.to_string())
                .map(|cwd| path_string(&cwd))
                .unwrap_or_default()
        },
    );
    for signal in signals {
        state.triggers.publish_internal(signal);
    }
}

```

7. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::` passes
- [ ] `grep -c "publish_signals" src-tauri/src/pty/mod.rs` prints `2`
- [ ] `grep -c "on_enter" src-tauri/src/pty/mod.rs` prints `1`
- [ ] `grep -c "tap::" src-tauri/src/pty/mod.rs` prints `5` (the `Session` field, the `default()` call, the `start_reader` parameter, the `Tap::new` call and the `publish_signals` parameter)
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- Any other feed (agent, task, pane opened, worktrees, review, notes, focus). Those are slice C.
- Calling `publish_signals` from the second (emit) thread, or scanning the coalesced batches. The tap is on the raw reads.
- Changing the webview's own bell and OSC handling, `report_cwd`, or any frontend file.
- Changing `restart` or `close`. A restarted session gets a new clock and a new tap through `spawn`.
- Adding a test that starts a Tauri app. `pty/shell/live.rs` (T17) proves the signals from a real shell.
- Editing docs. Fixing existing clippy warnings or renaming anything.

## If stuck

- If `publish_internal` or `wants_output` is not found on `TriggerService`, T08 is not done: set `status: blocked` and say so.
- If the build says `closure may outlive` or `cannot move out of tap_app` at the reader thread, the two `let` lines of step 6a were placed after `{ let pump = ...` instead of before it: move them up, as given.
- If `cargo test -p terminal-grid pty::` hangs for more than 60 seconds, a lock was taken in the wrong order (step 5): undo your change to `write` and re-apply it exactly as given, then set `status: blocked` if it still hangs.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: clippy baseline 7 warnings; built, acceptance passed
- 2026-10-12 checker: pass
