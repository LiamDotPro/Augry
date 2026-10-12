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
