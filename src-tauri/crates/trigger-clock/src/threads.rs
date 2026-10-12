//! The threads that join the wall clock and the calendar feeds to the pure
//! logic. Glue only: every decision is made in `logic.rs`, and the OS is
//! reached through `os.rs` and the `Fetcher` trait.
//!
//! `trigger-clock` is the clock thread. It blocks on one channel for as long
//! as `logic::wait` says, then reads the wall clock and calls `logic::step`.
//! The same channel carries the stop message, so stopping is immediate.
//!
//! `trigger-clock-feed` is the feed thread, started only when a rule names a
//! feed. It reads each distinct feed at once and then every `Parts::poll`,
//! expands the next 24 hours of events and sends them to the clock thread.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use calcard::common::timezone::Tz as CalTz;
use trigger_core::testkit::{Clock, SystemClock};
use trigger_core::{Event, Handle, Sink, SourceError};

use crate::feed::{self, Fetched, Fetcher};
use crate::logic::{self, Armed, ClockConfig, Occurrence, State};
use crate::os;

/// How often each feed is read (Decision 9).
pub const FEED_POLL: Duration = Duration::from_secs(300);
/// How far ahead events are expanded.
pub const FEED_WINDOW: Duration = Duration::from_secs(24 * 3600);
/// How far back events are expanded, so an event that ended a moment before
/// the app looked is still there for its firing.
pub const FEED_LOOKBACK: Duration = Duration::from_secs(15 * 60);

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
    /// Time between two reads of the same feed.
    pub poll: Duration,
    pub fetcher: Box<dyn Fetcher>,
    /// The zone for event times written without one.
    pub zone: CalTz,
}

impl Parts {
    /// The real wall clock, the real timings, HTTP through `ureq` and this
    /// machine's time zone.
    pub fn system() -> Parts {
        Parts {
            cfg: ClockConfig::default(),
            clock: Arc::new(SystemClock),
            poll: FEED_POLL,
            fetcher: Box::new(os::UreqFetcher::new()),
            zone: os::local_zone(),
        }
    }
}

/// Starts the clock thread for these rules, and the feed thread when a rule
/// names a feed. Dropping the returned handle stops both within a moment: it
/// sends them a stop message and does not wait. A request already in flight
/// ends by itself, at the latest at its 15 second timeout.
pub fn launch(armed: Vec<Armed>, sink: Sink, parts: Parts) -> Result<Handle, SourceError> {
    let (tx, rx) = mpsc::channel::<Msg>();
    // Planned here, before the thread exists, so the clock a caller moves
    // after `launch` returns can never be read as the starting time.
    let mut state = State::new(parts.cfg, parts.clock.now());
    step_and_send(&mut state, &armed, &sink, parts.clock.now());
    let keys = feed_keys(&armed);
    let Parts {
        clock,
        poll,
        fetcher,
        zone,
        ..
    } = parts;

    let clock_for_loop = clock.clone();
    std::thread::Builder::new()
        .name("trigger-clock".into())
        .spawn(move || clock_loop(state, armed, rx, sink, clock_for_loop))
        .map_err(|e| SourceError::Os(e.to_string()))?;

    let mut feed_stop = None;
    if !keys.is_empty() {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let job = FeedJob {
            keys,
            fetcher,
            zone,
            clock,
            poll,
        };
        let to_clock = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("trigger-clock-feed".into())
            .spawn(move || feed_loop(job, to_clock, stop_rx));
        if let Err(e) = spawned {
            let _ = tx.send(Msg::Stop);
            return Err(SourceError::Os(e.to_string()));
        }
        feed_stop = Some(stop_tx);
    }

    Ok(Handle::new(move || {
        let _ = tx.send(Msg::Stop);
        if let Some(stop) = feed_stop {
            let _ = stop.send(());
        }
    }))
}

/// The distinct `feed` strings of the event rules, exactly as written. They
/// are the keys of `State::feeds`.
fn feed_keys(armed: &[Armed]) -> Vec<String> {
    let keys: BTreeSet<&String> = armed
        .iter()
        .filter(|a| {
            matches!(
                a.kind.as_str(),
                "clock.event-starting" | "clock.event-ended"
            )
        })
        .filter_map(|a| a.parsed.when.feed.as_ref())
        .collect();
    keys.into_iter().cloned().collect()
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

/// What the feed thread needs.
struct FeedJob {
    keys: Vec<String>,
    fetcher: Box<dyn Fetcher>,
    zone: CalTz,
    clock: Arc<dyn Clock>,
    poll: Duration,
}

/// The last good copy of a feed and the validators the server gave with it.
#[derive(Default)]
struct Cached {
    text: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// Reads every feed, sends the expanded events to the clock thread, then waits
/// for the next poll or the stop message. Stops too when the clock thread is
/// gone.
fn feed_loop(job: FeedJob, to_clock: Sender<Msg>, stop: Receiver<()>) {
    let mut cache: BTreeMap<String, Cached> = BTreeMap::new();
    loop {
        for key in &job.keys {
            if !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                return;
            }
            let cached = cache.entry(key.clone()).or_default();
            refresh(key, job.fetcher.as_ref(), cached);
            let Some(text) = &cached.text else { continue };
            // Expanded again on every poll, even when the server said nothing
            // changed, because the 24 hour window has moved.
            let now = job.clock.now();
            let from = now.checked_sub(FEED_LOOKBACK).unwrap_or(now);
            let expansion = feed::expand(text, job.zone, from, now + FEED_WINDOW);
            let msg = Msg::Feed {
                feed: key.clone(),
                occurrences: expansion.occurrences,
            };
            if to_clock.send(msg).is_err() {
                return;
            }
        }
        match stop.recv_timeout(job.poll) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Updates `cached` from the feed: a file is read again, a URL is asked for
/// only what changed. A failure keeps the last good copy and is tried again
/// at the next poll. The reason is not logged, because a feed address often
/// holds a secret token.
fn refresh(feed: &str, fetcher: &dyn Fetcher, cached: &mut Cached) {
    match feed::feed_url(feed) {
        None => {
            if let Ok(text) = os::read_feed_file(Path::new(feed.trim())) {
                cached.text = Some(text);
            }
        }
        Some(url) => {
            let got = fetcher.fetch(
                &url,
                cached.etag.as_deref(),
                cached.last_modified.as_deref(),
            );
            if let Ok(Fetched::Body {
                text,
                etag,
                last_modified,
            }) = got
            {
                *cached = Cached {
                    text: Some(text),
                    etag,
                    last_modified,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests;
