//! Keeps one engine child process running and feeds its lines to the rules.
//! The child is behind `EngineProcess`, so tests use fakes. Pure of the OS.

use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use trigger_core::RuleId;

use crate::logic::{fire, parse_line, Armed, Engine, Payload};

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

/// Called once for each rule an engine event fires: (rule id, kind, payload).
pub type Emit = Arc<dyn Fn(RuleId, String, Payload) + Send + Sync>;

/// Sleeps for the delay before a restart, or less when `Stop` is set. Tests
/// pass one that does not sleep.
pub type Pause = Arc<dyn Fn(Duration, &Stop) + Send + Sync>;

/// The pause the app uses: really sleeps, and wakes early on stop.
pub fn real_pause(delay: Duration, stop: &Stop) {
    stop.wait(delay);
}

/// The child that is running now, so `stop` can kill it.
type Current = Arc<Mutex<Option<Arc<dyn Kill>>>>;

/// Runs the engine child on its own thread, restarts it when it exits, and
/// kills it on `stop` or drop.
pub struct Supervisor {
    stop: Arc<Stop>,
    current: Current,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    /// Starts the thread. It spawns the child, reads lines, calls `emit` for
    /// every rule an event fires, and spawns again (after `pause`) when the
    /// child ends or fails to start.
    pub fn start(
        process: Arc<dyn EngineProcess>,
        rules: Vec<Armed>,
        emit: Emit,
        pause: Pause,
    ) -> io::Result<Supervisor> {
        let stop = Arc::new(Stop::default());
        let current: Current = Arc::new(Mutex::new(None));
        let name = format!("docker-events-{}", process.engine().as_str());
        let thread = {
            let (stop, current) = (stop.clone(), current.clone());
            std::thread::Builder::new()
                .name(name)
                .spawn(move || run(&*process, &rules, &*emit, &*pause, &stop, &current))?
        };
        Ok(Supervisor {
            stop,
            current,
            thread: Some(thread),
        })
    }

    /// Kills the child, waits for it, and joins the thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.stop();
        let running = lock(&self.current).take();
        if let Some(kill) = running {
            kill.kill();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The thread body. A run that delivered at least one line counts as healthy
/// and starts the backoff over; a run that gave nothing makes the next wait
/// longer.
fn run(
    process: &dyn EngineProcess,
    rules: &[Armed],
    emit: &(dyn Fn(RuleId, String, Payload) + Send + Sync),
    pause: &(dyn Fn(Duration, &Stop) + Send + Sync),
    stop: &Stop,
    current: &Current,
) {
    let mut failures: u32 = 0;
    while !stop.is_stopped() {
        let mut got_line = false;
        if let Ok(mut lines) = process.spawn() {
            let kill = lines.killer();
            *lock(current) = Some(kill.clone());
            // `stop` may have run just before the line above published the
            // handle, and then it had nothing to kill.
            if stop.is_stopped() {
                kill.kill();
            }
            while let Some(line) = lines.next_line() {
                got_line = true;
                if let Some(mut event) = parse_line(&line) {
                    event.engine = process.engine();
                    for (id, kind, payload) in fire(rules, &event) {
                        emit(id, kind, payload);
                    }
                }
            }
            // Reap the child if it ended by itself.
            kill.kill();
            *lock(current) = None;
        }
        if stop.is_stopped() {
            break;
        }
        if got_line {
            failures = 0;
        }
        pause(backoff(failures), stop);
        if !got_line {
            failures = failures.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod fakes;
#[cfg(test)]
mod kill_tests;
#[cfg(test)]
mod tests;
