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
