---
id: T15
epic: 002-trigger-internal
title: Add the command clock and the tap that makes signals
status: todo
depends_on: [T14, T09]
size: M
model: haiku
files:
  - src-tauri/src/pty/tap.rs
  - src-tauri/src/pty/mod.rs
---

## Goal

`src-tauri/src/pty/tap.rs` exists with two pure parts, both unit tested. `CommandClock` times commands as Decision 9 says: the start is the first Enter after a prompt mark (or after the last command end), the end is the next command end mark. `Tap` owns one session's `Scanner`, reads a chunk, and returns the `InternalSignal`s in it: `CommandEnded` (with the clock's duration), `Bell` and, only when asked, `OutputLine` cut to 200 characters. Nothing calls it yet (T16 does).

## Read first

- `plans/002-trigger-internal/epic.md` Decisions 8, 9 and 10
- `src-tauri/src/pty/marks.rs` `Scanner::feed`, `Scan`, `Mark` (T14)
- `src-tauri/src/triggers/internal/mod.rs:1-60` `InternalSignal`, the variants `CommandEnded`, `OutputLine` and `Bell` (T01)
- `src-tauri/src/pty/mod.rs:1-8` the module list you add one line to

## Steps

1. Before changing anything, run `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` and write the number in the Log. It is the baseline for the clippy check in Acceptance.

2. In `src-tauri/src/pty/mod.rs`, find:

```rust
pub mod reader;
pub mod shell;
```

Replace with:

```rust
pub mod reader;
pub mod shell;
pub mod tap;
```

3. Create `src-tauri/src/pty/tap.rs` with exactly this content. Two design points to keep. The clock is locked for one statement at a time, because `PtyManager::write` (T16) holds the sessions lock while it takes the clock, and the `cwd` closure takes the sessions lock: holding the clock across `cwd` could deadlock. And `cwd` is a closure called per signal, so a chunk of plain text costs no lookup.

```rust
//! Turns the bytes a shell prints into signals for the internal triggers:
//! command ended (with how long it ran), bell, and output lines. The reader
//! thread owns one `Tap` per session; `write` tells the shared `CommandClock`
//! when Enter is pressed. Pure, so the timing and the signals are tested here
//! without a window or a shell. See docs/technical-design.md section 4.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::pty::marks::{Mark, Scanner};
use crate::triggers::internal::InternalSignal;

/// The longest output line a signal carries, in characters.
pub const MAX_LINE_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, Default)]
enum ClockState {
    /// No prompt has been drawn yet, so Enter is not a command.
    #[default]
    NoPrompt,
    /// At a prompt: the next Enter starts a command.
    AtPrompt,
    Running(Instant),
}

/// Times commands. A command starts at the first Enter after a prompt mark
/// (or after the last command end) and ends at the next command end mark.
/// Shared between `PtyManager::write`, which calls `on_enter`, and the reader
/// thread, which calls the other two.
#[derive(Debug, Default)]
pub struct CommandClock {
    state: ClockState,
}

impl CommandClock {
    /// A prompt is being drawn. If a command was marked as running (an empty
    /// line, or a shell without end marks) it is forgotten.
    pub fn on_prompt(&mut self) {
        self.state = ClockState::AtPrompt;
    }

    /// The user pressed Enter. Only the first Enter after a prompt counts, so
    /// typing into a running program does not restart the clock.
    pub fn on_enter(&mut self, now: Instant) {
        if matches!(self.state, ClockState::AtPrompt) {
            self.state = ClockState::Running(now);
        }
    }

    /// A command ended. Returns how long it ran in milliseconds, or 0 when
    /// the start was never seen.
    pub fn on_command_end(&mut self, now: Instant) -> u64 {
        let elapsed = match self.state {
            ClockState::Running(start) => now.saturating_duration_since(start).as_millis() as u64,
            _ => 0,
        };
        self.state = ClockState::AtPrompt;
        elapsed
    }
}

/// One session's scanner plus the clock it shares with `write`.
pub struct Tap {
    pane: String,
    scanner: Scanner,
    clock: Arc<Mutex<CommandClock>>,
}

impl Tap {
    pub fn new(pane: String, clock: Arc<Mutex<CommandClock>>) -> Self {
        Tap {
            pane,
            scanner: Scanner::new(),
            clock,
        }
    }

    /// Reads one chunk of output and returns the signals it holds. `cwd` gives
    /// the pane's working directory and is called once per signal that needs
    /// it, so a chunk of plain text costs no lookups. Lines are only read when
    /// `want_lines` is true. Each line is cut to `MAX_LINE_CHARS`.
    pub fn process(
        &mut self,
        bytes: &[u8],
        want_lines: bool,
        now: Instant,
        cwd: impl Fn(&str) -> String,
    ) -> Vec<InternalSignal> {
        let scan = self.scanner.feed(bytes, want_lines);
        let mut signals = Vec::new();
        for mark in scan.marks {
            // The clock is locked for one statement at a time: `write` holds the
            // sessions lock while it takes the clock, and `cwd` takes the sessions
            // lock, so holding both here could deadlock.
            match mark {
                Mark::PromptStart => self.clock.lock().expect("pty clock").on_prompt(),
                Mark::CommandEnd { exit } => {
                    let duration_ms = self.clock.lock().expect("pty clock").on_command_end(now);
                    signals.push(InternalSignal::CommandEnded {
                        pane: self.pane.clone(),
                        cwd: cwd(&self.pane),
                        exit_code: exit,
                        duration_ms,
                    });
                }
                Mark::Bell => signals.push(InternalSignal::Bell {
                    pane: self.pane.clone(),
                }),
            }
        }
        for line in scan.lines {
            signals.push(InternalSignal::OutputLine {
                pane: self.pane.clone(),
                cwd: cwd(&self.pane),
                line: line.chars().take(MAX_LINE_CHARS).collect(),
            });
        }
        signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tap() -> (Tap, Arc<Mutex<CommandClock>>) {
        let clock = Arc::new(Mutex::new(CommandClock::default()));
        (Tap::new("p1".to_string(), Arc::clone(&clock)), clock)
    }

    fn cwd(_: &str) -> String {
        "/work".to_string()
    }

    const END_1: &[u8] = b"\x1b]7777;cmd;ZmFsc2U=\x07\x1b]133;D;1\x07\x1b]133;A\x07";

    #[test]
    fn the_clock_runs_from_enter_after_a_prompt_to_the_command_end() {
        let t0 = Instant::now();
        let mut clock = CommandClock::default();
        clock.on_enter(t0);
        clock.on_prompt();
        clock.on_enter(t0 + Duration::from_millis(100));
        // A second Enter while running (input to the program) changes nothing.
        clock.on_enter(t0 + Duration::from_millis(900));
        assert_eq!(clock.on_command_end(t0 + Duration::from_millis(2100)), 2000);
    }

    #[test]
    fn a_command_end_never_started_lasts_zero_and_arms_the_next_enter() {
        let t0 = Instant::now();
        let mut clock = CommandClock::default();
        assert_eq!(clock.on_command_end(t0), 0);
        clock.on_enter(t0);
        assert_eq!(clock.on_command_end(t0 + Duration::from_millis(5)), 5);
    }

    #[test]
    fn an_empty_line_is_forgotten_at_the_next_prompt() {
        let t0 = Instant::now();
        let mut clock = CommandClock::default();
        clock.on_prompt();
        clock.on_enter(t0);
        clock.on_prompt();
        clock.on_enter(t0 + Duration::from_secs(10));
        assert_eq!(clock.on_command_end(t0 + Duration::from_secs(11)), 1000);
    }

    #[test]
    fn a_command_end_becomes_a_signal_with_its_duration() {
        let (mut tap, clock) = tap();
        let t0 = Instant::now();
        tap.process(b"\x1b]133;A\x07", false, t0, cwd);
        clock.lock().unwrap().on_enter(t0);
        let signals = tap.process(END_1, false, t0 + Duration::from_millis(1500), cwd);
        assert_eq!(
            signals,
            vec![InternalSignal::CommandEnded {
                pane: "p1".to_string(),
                cwd: "/work".to_string(),
                exit_code: 1,
                duration_ms: 1500,
            }]
        );
    }

    #[test]
    fn a_bell_is_a_signal_and_a_prompt_alone_is_not() {
        let (mut tap, _) = tap();
        let now = Instant::now();
        assert!(tap.process(b"\x1b]133;A\x07$ ", false, now, cwd).is_empty());
        assert_eq!(
            tap.process(b"\x07", false, now, cwd),
            vec![InternalSignal::Bell {
                pane: "p1".to_string()
            }]
        );
    }

    #[test]
    fn lines_become_signals_only_when_wanted_and_are_cut() {
        let (mut tap, _) = tap();
        let now = Instant::now();
        assert!(tap.process(b"error: boom\n", false, now, cwd).is_empty());
        let mut long = "\u{e9}".repeat(300).into_bytes();
        long.push(b'\n');
        let signals = tap.process(&long, true, now, cwd);
        match &signals[..] {
            [InternalSignal::OutputLine { pane, cwd, line }] => {
                assert_eq!(pane, "p1");
                assert_eq!(cwd, "/work");
                assert_eq!(line.chars().count(), MAX_LINE_CHARS);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn cwd_is_not_looked_up_for_plain_text_or_a_prompt() {
        let (mut tap, _) = tap();
        let asked = std::cell::Cell::new(0);
        let count = |_: &str| {
            asked.set(asked.get() + 1);
            String::new()
        };
        tap.process(b"hello\n\x1b]133;A\x07", false, Instant::now(), count);
        assert_eq!(asked.get(), 0);
    }

    #[test]
    fn a_split_end_mark_gives_one_signal() {
        let (mut tap, _) = tap();
        let now = Instant::now();
        let (a, b) = END_1.split_at(END_1.len() - 12);
        let mut signals = tap.process(a, false, now, cwd);
        signals.extend(tap.process(b, false, now, cwd));
        assert_eq!(signals.len(), 1);
    }
}
```

4. Run `cd src-tauri && cargo fmt`. If it changes the code above, keep the result.

## Acceptance

- [ ] `cd src-tauri && cargo test -p terminal-grid pty::tap` passes and reports 8 passed
- [ ] `grep -c "InternalSignal::" src-tauri/src/pty/tap.rs` prints a number greater than 0
- [ ] `cd src-tauri && cargo test --workspace` passes
- [ ] `cd src-tauri && cargo fmt --check` exits 0
- [ ] `cd src-tauri && cargo check -p terminal-grid --no-default-features` exits 0
- [ ] `cd src-tauri && cargo clippy -p terminal-grid --all-targets 2>&1 | grep -c warning` prints the same number as the baseline in the Log, or a lower one
- [ ] `git status --porcelain | grep -v 'plans/002-trigger-internal/tasks/'` lists only paths under `files:`

## Out of scope

- Adding the clock to `Session`, calling `on_enter` from `write`, or starting a `Tap` in the reader thread. That is T16.
- Publishing signals. `Tap::process` only returns them.
- Changing `marks.rs` or `InternalSignal`.
- A `Duration` type for the milliseconds. `duration_ms` is a `u64` as in `InternalSignal`.
- Editing docs. Fixing existing clippy warnings or renaming anything.

## If stuck

- If `use crate::triggers::internal::InternalSignal` does not resolve, T01 is not done or `InternalSignal` is not `pub`: set `status: blocked` and say so in the Log.
- If `InternalSignal::CommandEnded` has other field names than `pane`, `cwd`, `exit_code`, `duration_ms` (check `src-tauri/src/triggers/internal/mod.rs`), do not rename them here: set `status: blocked`.
- If a test fails, do not change the expected values. Re-copy the code from step 3.
- If `cargo fmt` changes code given above, keep the formatted result; that is expected.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
