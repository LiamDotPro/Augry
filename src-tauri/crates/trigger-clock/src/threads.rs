//! The threads that join the wall clock and the calendar feeds to the pure
//! logic. Glue only: every decision is made in `logic.rs`, and the OS is
//! reached through `os.rs` and the `Fetcher` trait.
//!
//! `trigger-clock` is the clock thread. It blocks on one channel for as long
//! as `logic::wait` says, then reads the wall clock and calls `logic::step`.
//! The same channel carries the stop message, so stopping is immediate.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::SystemTime;

use trigger_core::testkit::{Clock, SystemClock};
use trigger_core::{Event, Handle, Sink, SourceError};

use crate::logic::{self, Armed, ClockConfig, Occurrence, State};

/// What the clock thread can be told.
pub enum Msg {
    /// Stop the thread.
    Stop,
    /// The feed thread's latest occurrences for one feed, keyed by the `feed`
    /// field exactly as the rules wrote it.
    Feed {
        feed: String,
        occurrences: Vec<Occurrence>,
    },
}

/// Everything the threads take from outside, so a test can replace it.
pub struct Parts {
    pub cfg: ClockConfig,
    pub clock: Arc<dyn Clock>,
}

impl Parts {
    /// The real wall clock and the real timings.
    pub fn system() -> Parts {
        Parts {
            cfg: ClockConfig::default(),
            clock: Arc::new(SystemClock),
        }
    }
}

/// Starts the clock thread for these rules. Dropping the returned handle
/// stops it within a moment: it sends `Msg::Stop` and does not wait.
pub fn launch(armed: Vec<Armed>, sink: Sink, parts: Parts) -> Result<Handle, SourceError> {
    let (tx, rx) = mpsc::channel::<Msg>();
    // Planned here, before the thread exists, so the clock a caller moves
    // after `launch` returns can never be read as the starting time.
    let mut state = State::new(parts.cfg, parts.clock.now());
    step_and_send(&mut state, &armed, &sink, parts.clock.now());
    let clock = parts.clock;
    std::thread::Builder::new()
        .name("trigger-clock".into())
        .spawn(move || clock_loop(state, armed, rx, sink, clock))
        .map_err(|e| SourceError::Os(e.to_string()))?;
    Ok(Handle::new(move || {
        let _ = tx.send(Msg::Stop);
    }))
}

/// Waits, steps and sends what fired, until told to stop or the sink is
/// closed.
fn clock_loop(
    mut state: State,
    armed: Vec<Armed>,
    rx: Receiver<Msg>,
    sink: Sink,
    clock: Arc<dyn Clock>,
) {
    loop {
        match rx.recv_timeout(logic::wait(&state, clock.now())) {
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Msg::Feed { feed, occurrences }) => {
                state.feeds.insert(feed, occurrences);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if !step_and_send(&mut state, &armed, &sink, clock.now()) {
            return;
        }
    }
}

/// One step with the machine's local time zone. Returns false when the sink
/// is closed, which ends the thread.
fn step_and_send(state: &mut State, armed: &[Armed], sink: &Sink, now: SystemTime) -> bool {
    for fire in logic::step(state, &chrono::Local, now, armed) {
        let event = Event {
            rule: fire.rule,
            kind: fire.kind,
            at: now,
            payload: fire.payload,
        };
        if sink.send(event).is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests;
