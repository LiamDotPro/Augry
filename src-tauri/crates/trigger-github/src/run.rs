//! The thread behind `GithubSource::start`: joins `gh` to the pure poller and
//! the sink. Glue only. What to ask, when, and what fires is decided in
//! `logic/poll.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use trigger_core::{Event, Handle, Sink, SourceError};

use crate::logic::cli::Gh;
use crate::logic::poll::Poller;
use crate::logic::Armed;
use crate::os;

/// Shown on every rule when `gh` is installed but not signed in.
pub const NOT_SIGNED_IN: &str = "Run gh auth login in a terminal";

/// The time in whole seconds since the Unix epoch. A test passes its own.
pub type Clock = Box<dyn Fn() -> u64 + Send>;

/// Decides whether the source can start. `found` is where `gh` is and `auth`
/// asks it whether it is signed in. `gh` missing or not signed in is
/// `Unavailable` with the thing to do; a failed question is `Os`.
pub fn check_login(
    found: Option<&Path>,
    auth: &dyn Fn(&Path) -> Result<bool, String>,
) -> Result<PathBuf, SourceError> {
    let exe = found.ok_or_else(|| SourceError::Unavailable(os::NOT_FOUND.to_string()))?;
    match auth(exe) {
        Ok(true) => Ok(exe.to_path_buf()),
        Ok(false) => Err(SourceError::Unavailable(NOT_SIGNED_IN.to_string())),
        Err(why) => Err(SourceError::Os(why)),
    }
}

/// Starts the thread `trigger-github`. It asks `gh` about whatever is due
/// every second and sends what fires to `sink`. Dropping the handle stops it:
/// at the latest after the request in flight, and at once if it is waiting.
pub fn spawn(
    armed: Vec<Armed>,
    gh: Box<dyn Gh>,
    sink: Sink,
    clock: Clock,
) -> Result<Handle, SourceError> {
    let poller = Poller::new(armed, clock());
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("trigger-github".into())
        .spawn(move || run(poller, gh, sink, clock, &thread_stop))
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(Handle::new(move || stop.store(true, Ordering::Relaxed)))
}

fn run(mut poller: Poller, gh: Box<dyn Gh>, sink: Sink, clock: Clock, stop: &AtomicBool) {
    let cancelled = || stop.load(Ordering::Relaxed);
    while !cancelled() {
        let outcome = poller.tick(clock(), gh.as_ref(), &cancelled);
        for note in &outcome.notes {
            eprintln!("trigger-github: {}: {}", note.rules.join(", "), note.text);
        }
        for fire in outcome.fires {
            if cancelled() {
                return;
            }
            let event = Event {
                rule: fire.rule,
                kind: fire.kind,
                at: SystemTime::now(),
                payload: fire.payload,
            };
            if sink.send(event).is_err() {
                return;
            }
        }
        if poller.is_stopped() {
            return;
        }
        // Wake every second, but look at the stop flag ten times in it.
        for _ in 0..10 {
            if cancelled() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests;
