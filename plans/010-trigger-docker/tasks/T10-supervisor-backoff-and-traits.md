---
id: T10
epic: 010-trigger-docker
title: Add the supervisor backoff and process traits
status: done
depends_on: [T08]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/lib.rs
  - src-tauri/crates/trigger-docker/src/supervisor.rs
  - src-tauri/crates/trigger-docker/src/supervisor/tests.rs
---

## Goal

The crate has `src/supervisor.rs` with the pure `backoff(attempt)` function (1, 2, 4, 8, 16, 32 then 60 seconds) and the three traits the supervisor is built on: `EngineProcess`, `Lines` and `Kill`. Tests check the backoff numbers.

## Read first

- `plans/010-trigger-docker/epic.md` Decision 9 (the supervisor) and the Risks line about stopping the child
- `plans/010-trigger-docker/research/R02-engine-binary.md` the part about killing the child: `child.kill()` then `child.wait()`
- `src-tauri/crates/trigger-docker/src/logic.rs` the `Engine` enum (`Docker`, `Podman`, `as_str`). Not `rule::EngineChoice`.
- `src-tauri/crates/trigger-docker/src/lib.rs` the `mod` lines at the top

## Steps

1. In `src-tauri/crates/trigger-docker/src/lib.rs` find the line `mod rule;` and add a line directly under it so the three lines read:

```rust
mod rule;
mod supervisor;
```

(`mod logic;` and `mod os;` above stay as they are.)

2. Create `src-tauri/crates/trigger-docker/src/supervisor.rs` with exactly this content. The names are a contract for later cards: `EngineProcess` is the epic's `Engine` trait, renamed so it does not clash with the `Engine` enum in `logic.rs`. `Kill` exists because the thread that reads lines is blocked inside `next_line`, so a second handle is needed to kill the child from outside.

```rust
//! Keeps one engine child process running and feeds its lines to the rules.
//! The child is behind `EngineProcess`, so tests use fakes. Pure of the OS.

// Removed when lib.rs reads everything in this file (slice C).
#![allow(dead_code)]

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crate::logic::Engine;

/// How long to wait before starting the child again. `attempt` counts the
/// failed runs in a row, from 0: 1, 2, 4, 8, 16, 32, then 60 seconds for ever.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(6)).min(Duration::from_secs(60))
}

/// Kills a running child. Another thread may be blocked in `Lines::next_line`
/// when this is called, and that call must return `None` soon after.
pub trait Kill: Send + Sync {
    /// Kills the child, then waits for it so it does not stay a zombie. Safe to
    /// call twice and after the child has already exited.
    fn kill(&self);
}

/// The output of one running child.
pub trait Lines: Send {
    /// The next line of stdout without its line ending. Blocks. `None` means
    /// the child has exited or was killed.
    fn next_line(&mut self) -> Option<String>;
    /// A handle for killing this child from another thread.
    fn killer(&self) -> Arc<dyn Kill>;
}

/// Something that can start the engine's event stream, over and over.
pub trait EngineProcess: Send + Sync {
    /// The engine this process talks to. The supervisor stamps it on every
    /// event, so a rule that names an engine matches what really ran.
    fn engine(&self) -> Engine;
    /// Starts a new child.
    fn spawn(&self) -> io::Result<Box<dyn Lines>>;
}

#[cfg(test)]
mod tests;
```

3. Create the folder `src-tauri/crates/trigger-docker/src/supervisor/` and in it `tests.rs` with exactly this content.

```rust
use super::*;

#[test]
fn backoff_doubles_from_one_second() {
    let seconds: Vec<u64> = (0..6).map(|n| backoff(n).as_secs()).collect();
    assert_eq!(seconds, vec![1, 2, 4, 8, 16, 32]);
}

#[test]
fn backoff_stops_at_sixty_seconds() {
    assert_eq!(backoff(6), Duration::from_secs(60));
    assert_eq!(backoff(7), Duration::from_secs(60));
    assert_eq!(backoff(1000), Duration::from_secs(60));
    assert_eq!(backoff(u32::MAX), Duration::from_secs(60));
}
```

4. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 2 new tests in the library (`backoff_doubles_from_one_second`, `backoff_stops_at_sixty_seconds`) on top of slice A's
- [ ] `cd src-tauri && cargo clippy -p trigger-docker --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-docker --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0 (it may print warnings for template text in `README.md`, `src/lib.rs` and `src/os.rs`; those stay until slice C)
- [ ] `git status --porcelain` lists only paths under `files:`, ignoring other card files under `plans/010-trigger-docker/tasks/`

## Out of scope

- The README. The supervisor is internal and nothing reads it until slice C, which writes the README parts that mention it.
- Editing `src/logic.rs`, `src/rule.rs`, `src/os.rs` or any slice A test. Use their names as they are.
- Adding dependencies. The supervisor uses `std` only.
- Writing the real `std::process` engine, `find_engine` or the version check (slice C).
- Removing the `#![allow(dead_code)]` line in `supervisor.rs`. Slice C removes it.
- Registering the crate in `src-tauri/Cargo.toml` or `sources()` (slice D).
- Anything that spawns a thread or a process. Later cards do that.

## If stuck

- If `src/lib.rs` has no `mod rule;` line, set status blocked and say what the top of the file looks like.
- If `crate::logic::Engine` does not exist, set status blocked. Slice A (T03) defines it; do not define your own.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
