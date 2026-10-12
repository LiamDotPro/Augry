---
id: T11
epic: 010-trigger-docker
title: Add the Stop flag that wakes a sleeping supervisor
status: todo
depends_on: [T10]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-docker/src/supervisor.rs
  - src-tauri/crates/trigger-docker/src/supervisor/tests.rs
---

## Goal

The supervisor file has `Stop`, a stop flag with a timed `wait` that wakes the moment `stop()` is called, so a 60 second backoff never delays shutdown. Three tests cover it.

## Read first

- `src-tauri/crates/trigger-docker/src/supervisor.rs` the file as T10 left it
- `src-tauri/crates/trigger-docker/src/supervisor/tests.rs` the tests so far
- `plans/_context/conventions.md` the rule: no `unwrap` or `expect` in library code

## Steps

1. Replace the whole of `src-tauri/crates/trigger-docker/src/supervisor.rs` with this. Compared with T10 there is one new import line change (`Condvar`, `Mutex`, `MutexGuard`, `PoisonError` join `Arc`), the `lock` helper (it recovers from a poisoned mutex instead of using `unwrap`) and `Stop`.

```rust
//! Keeps one engine child process running and feeds its lines to the rules.
//! The child is behind `EngineProcess`, so tests use fakes. Pure of the OS.

// Removed when lib.rs reads everything in this file (slice C).
#![allow(dead_code)]

use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
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

/// Locks a mutex even if another thread panicked while holding it. The data
/// here is a flag or a handle, so it is never half written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A flag that says "stop now" and wakes anyone sleeping on it. Shared by the
/// caller and the supervisor thread.
#[derive(Default)]
pub struct Stop {
    stopped: Mutex<bool>,
    changed: Condvar,
}

impl Stop {
    pub fn is_stopped(&self) -> bool {
        *lock(&self.stopped)
    }

    /// Sets the flag and wakes every `wait`.
    pub fn stop(&self) {
        *lock(&self.stopped) = true;
        self.changed.notify_all();
    }

    /// Sleeps for `delay`, or less if `stop` is called meanwhile. Returns true
    /// if the flag is set.
    pub fn wait(&self, delay: Duration) -> bool {
        let guard = lock(&self.stopped);
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, delay, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

#[cfg(test)]
mod tests;
```

2. Append this to the end of `src-tauri/crates/trigger-docker/src/supervisor/tests.rs` after a blank line.

```rust

#[test]
fn stop_wakes_a_long_wait_at_once() {
    let stop = Arc::new(Stop::default());
    assert!(!stop.is_stopped());
    let waiter = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let stopped = stop.wait(Duration::from_secs(60));
            (stopped, started.elapsed())
        })
    };
    std::thread::sleep(Duration::from_millis(50));
    stop.stop();
    let (stopped, elapsed) = waiter.join().unwrap();
    assert!(stopped);
    assert!(elapsed < Duration::from_secs(5));
    assert!(stop.is_stopped());
}

#[test]
fn wait_times_out_when_nobody_stops() {
    let stop = Stop::default();
    assert!(!stop.wait(Duration::from_millis(10)));
}

#[test]
fn wait_returns_at_once_when_already_stopped() {
    let stop = Stop::default();
    stop.stop();
    assert!(stop.wait(Duration::from_secs(60)));
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-docker`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-docker` passes: 5 tests in the library under `supervisor::tests` (2 from T10, 3 new) on top of slice A's
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
- Anything that spawns a process. `Stop` is only a flag and a condvar.

## If stuck

- If `Stop` or `lock` already exists in `supervisor.rs`, set status blocked and say so.
- If `cargo fmt -p trigger-docker --check` fails, run `cd src-tauri && cargo fmt -p trigger-docker` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
