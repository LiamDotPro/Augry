---
id: T21
epic: 007-trigger-clock
title: Start the feed thread and send events to the clock thread
status: done
depends_on: [T20]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/threads.rs
  - src-tauri/crates/trigger-clock/src/threads/tests.rs
---

## Goal

When a rule names a feed, `launch` also starts the `trigger-clock-feed` thread. It reads each distinct feed at once and then every 5 minutes (a URL with `UreqFetcher` and conditional requests, a path with `read_feed_file`), expands the next 24 hours with the local zone, and sends the occurrences to the clock thread, which fires the event kinds from them. Dropping the handle stops both threads within a second. Tests use a fake `Fetcher` and a 50 millisecond poll.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 9 (feed thread, 5 minutes, what is sent)
- `src-tauri/crates/trigger-clock/src/threads.rs` the T20 file you replace; `Msg::Feed` is already handled by `clock_loop`
- `src-tauri/crates/trigger-clock/src/feed.rs` `Fetcher`, `Fetched`, `feed_url`, `expand` and `Expansion` (do not edit)
- `src-tauri/crates/trigger-clock/src/os.rs` `UreqFetcher`, `read_feed_file`, `local_zone` (do not edit)
- `src-tauri/crates/trigger-clock/src/logic/events.rs` how `fire_events` reads `State::feeds` (keyed by the rule's `feed` text exactly as written) and fires each occurrence once (do not edit)

## Steps

1. Replace the whole of `src-tauri/crates/trigger-clock/src/threads.rs` with this. It is the T20 file with the feed thread added. What to know before pasting: the feed keys are the `feed` strings exactly as the rules wrote them, because `State::feeds` is looked up by that text; a failed fetch keeps the last good copy and is not logged, because feed addresses often hold a secret token; the window starts 15 minutes in the past so an event that ended just before the app looked can still fire `clock.event-ended`.

```rust
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
```

2. Replace the whole of `src-tauri/crates/trigger-clock/src/threads/tests.rs` with this. It keeps the six T20 tests (their `parts` helper now fills the new fields) and adds six more.

```rust
use super::*;
use std::collections::VecDeque;
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;
use trigger_core::testkit::{rule, FixedClock, RecordingSink};
use trigger_core::{Rule, Source};

use crate::logic::millis;
use crate::ClockSource;

const URL: &str = "https://calendar.test/team.ics";
const MODIFIED: &str = "Tue, 13 Oct 2026 08:00:00 GMT";

/// What a fetch was asked: the URL, the `etag` and the `last_modified`.
type Seen = Arc<Mutex<Vec<(String, Option<String>, Option<String>)>>>;

/// Answers from a list, then with `NotModified` for ever. Records every
/// question. Holds an `Arc` that other code can count to see when the feed
/// thread (which owns the fetcher) has ended.
struct FakeFetcher {
    replies: Mutex<VecDeque<Result<Fetched, String>>>,
    seen: Seen,
    alive: Arc<()>,
}

impl FakeFetcher {
    fn new(replies: Vec<Result<Fetched, String>>) -> FakeFetcher {
        FakeFetcher {
            replies: Mutex::new(replies.into()),
            seen: Seen::default(),
            alive: Arc::new(()),
        }
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<Fetched, String> {
        self.seen.lock().unwrap().push((
            url.to_string(),
            etag.map(str::to_string),
            last_modified.map(str::to_string),
        ));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Fetched::NotModified))
    }
}

/// A calendar with one event, times in UTC.
fn ics(title: &str, start: SystemTime, end: SystemTime) -> String {
    let stamp = |t: SystemTime| DateTime::<Utc>::from(t).format("%Y%m%dT%H%M%SZ");
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//fixture//EN\r\nBEGIN:VEVENT\r\n\
         UID:one@fixture\r\nDTSTAMP:20260101T000000Z\r\nDTSTART:{}\r\nDTEND:{}\r\n\
         SUMMARY:{title}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        stamp(start),
        stamp(end)
    )
}

fn body(text: String) -> Result<Fetched, String> {
    Ok(Fetched::Body {
        text,
        etag: Some("\"v1\"".into()),
        last_modified: Some(MODIFIED.into()),
    })
}

/// A clock at `t0`, a fetcher that answers `replies`, and a 50 ms poll.
fn feed_parts(clock: &Arc<FixedClock>, fetcher: FakeFetcher) -> Parts {
    Parts {
        poll: Duration::from_millis(50),
        fetcher: Box::new(fetcher),
        ..parts(clock)
    }
}

fn starting(feed: &str) -> Vec<Rule> {
    vec![rule("p/e", "clock.event-starting", json!({ "feed": feed }))]
}

/// 2026-10-13 12:00 UTC. The tests move a `FixedClock` from here.
fn t0() -> SystemTime {
    Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap().into()
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Short timings so a stopped or moved clock is noticed within 20 ms.
fn short_cfg() -> ClockConfig {
    ClockConfig {
        tick: Duration::from_millis(20),
        jump: secs(90),
        normal_for: secs(60),
    }
}

fn clock_at(t: SystemTime) -> Arc<FixedClock> {
    Arc::new(FixedClock(Mutex::new(t)))
}

fn set(clock: &FixedClock, t: SystemTime) {
    *clock.0.lock().unwrap() = t;
}

fn armed(rules: Vec<Rule>) -> Vec<Armed> {
    rules
        .iter()
        .map(|r| Armed {
            id: r.id.clone(),
            kind: r.kind.clone(),
            parsed: crate::rule::parse(r).unwrap(),
        })
        .collect()
}

fn parts(clock: &Arc<FixedClock>) -> Parts {
    Parts {
        cfg: short_cfg(),
        clock: clock.clone(),
        poll: secs(3600),
        fetcher: Box::new(FakeFetcher::new(Vec::new())),
        zone: CalTz::Floating,
    }
}

#[test]
fn an_interval_fires_when_the_clock_reaches_it() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 10 }))];
    let _handle = launch(armed(rules), sink.tx.clone(), parts(&clock)).unwrap();
    // Nothing is due while the clock stands still.
    assert!(sink
        .wait_for("clock.interval", Duration::from_millis(150))
        .is_none());
    set(&clock, t0() + secs(10));
    let event = sink.wait_for("clock.interval", secs(2)).expect("fired");
    assert_eq!(event.rule, "p/i");
    assert_eq!(event.payload["due"], json!(millis(t0() + secs(10))));
    assert_eq!(event.payload["caught_up"], json!(false));
    assert_eq!(event.at, t0() + secs(10));
}

#[test]
fn a_long_sleep_fires_once_with_catch_up_and_not_at_all_without() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let rules = vec![
        rule(
            "p/with",
            "clock.interval",
            json!({ "every_s": 3600, "catch_up": true }),
        ),
        rule("p/without", "clock.interval", json!({ "every_s": 3600 })),
    ];
    let _handle = launch(armed(rules), sink.tx.clone(), parts(&clock)).unwrap();
    // Six hours pass in one reading: the machine slept.
    set(&clock, t0() + secs(6 * 3600));
    let event = sink.wait_for("clock.interval", secs(2)).expect("fired");
    assert_eq!(event.rule, "p/with");
    assert_eq!(event.payload["caught_up"], json!(true));
    assert_eq!(event.payload["due"], json!(millis(t0() + secs(6 * 3600))));
    std::thread::sleep(Duration::from_millis(150));
    assert!(sink.drain().is_empty());
}

#[test]
fn dropping_the_handle_stops_the_thread_within_a_second() {
    let clock = clock_at(t0());
    let (tx, rx) = channel::<Event>();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 10 }))];
    let handle = launch(armed(rules), tx, parts(&clock)).unwrap();
    drop(handle);
    // The thread owned the only other sender, so it is gone when this reports
    // a disconnect.
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn the_stop_is_not_held_up_by_a_long_tick() {
    let clock = clock_at(t0());
    let (tx, rx) = channel::<Event>();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 3600 }))];
    let slow = Parts {
        cfg: ClockConfig::default(),
        ..parts(&clock)
    };
    let handle = launch(armed(rules), tx, slow).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn start_names_the_rule_that_does_not_parse() {
    let (tx, _rx) = channel();
    let bad = rule("p/bad", "clock.interval", json!({ "every_s": 1 }));
    match ClockSource.start(vec![bad], tx) {
        Err(SourceError::Os(text)) => assert!(text.starts_with("p/bad: ")),
        other => panic!("expected an Os error, got {:?}", other.err()),
    }
}

#[test]
fn start_runs_with_the_real_clock_and_stops_on_drop() {
    let (tx, rx) = channel();
    let good = rule("p/i", "clock.interval", json!({ "every_s": 3600 }));
    let handle = ClockSource.start(vec![good], tx).unwrap();
    drop(handle);
    assert!(matches!(
        rx.recv_timeout(secs(1)),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn an_event_from_a_fetched_feed_fires_once_and_the_next_poll_asks_for_changes_only() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    // Starts in 3 minutes; the rule fires 5 minutes before, so it is due now.
    let text = ics("Planning", t0() + secs(180), t0() + secs(2 * 3600));
    let fetcher = FakeFetcher::new(vec![body(text)]);
    let seen = fetcher.seen.clone();
    let _handle = launch(
        armed(starting(URL)),
        sink.tx.clone(),
        feed_parts(&clock, fetcher),
    )
    .unwrap();
    let event = sink
        .wait_for("clock.event-starting", secs(3))
        .expect("fired");
    assert_eq!(event.rule, "p/e");
    assert_eq!(event.payload["title"], json!("Planning"));
    assert_eq!(event.payload["start"], json!(millis(t0() + secs(180))));
    assert_eq!(event.payload["end"], json!(millis(t0() + secs(2 * 3600))));
    assert_eq!(event.payload["feed"], json!(URL));
    // Let several polls pass: the same event is sent again and not re-fired.
    std::thread::sleep(Duration::from_millis(400));
    assert!(sink.drain().is_empty());
    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 3, "polled {} times", seen.len());
    assert_eq!(seen[0], (URL.to_string(), None, None));
    assert_eq!(
        seen[1],
        (
            URL.to_string(),
            Some("\"v1\"".to_string()),
            Some(MODIFIED.to_string())
        )
    );
}

#[test]
fn a_webcal_feed_is_fetched_as_https_and_reported_as_written() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let text = ics("Planning", t0() + secs(180), t0() + secs(3600));
    let fetcher = FakeFetcher::new(vec![body(text)]);
    let seen = fetcher.seen.clone();
    let feed = "webcal://calendar.test/team.ics";
    let _handle = launch(
        armed(starting(feed)),
        sink.tx.clone(),
        feed_parts(&clock, fetcher),
    )
    .unwrap();
    let event = sink
        .wait_for("clock.event-starting", secs(3))
        .expect("fired");
    assert_eq!(event.payload["feed"], json!(feed));
    assert_eq!(seen.lock().unwrap()[0].0, URL);
}

#[test]
fn a_failed_fetch_is_tried_again_at_the_next_poll() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let text = ics("Planning", t0() + secs(180), t0() + secs(3600));
    let fetcher = FakeFetcher::new(vec![Err("the request failed".into()), body(text)]);
    let _handle = launch(
        armed(starting(URL)),
        sink.tx.clone(),
        feed_parts(&clock, fetcher),
    )
    .unwrap();
    assert!(sink.wait_for("clock.event-starting", secs(3)).is_some());
}

#[test]
fn a_feed_given_as_a_path_is_read_from_disk_and_never_fetched() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let path = std::env::temp_dir().join(format!("trigger-clock-{}.ics", uuid::Uuid::new_v4()));
    std::fs::write(&path, ics("Planning", t0() + secs(180), t0() + secs(3600))).unwrap();
    let fetcher = FakeFetcher::new(Vec::new());
    let seen = fetcher.seen.clone();
    let feed = path.to_str().unwrap().to_string();
    let _handle = launch(
        armed(starting(&feed)),
        sink.tx.clone(),
        feed_parts(&clock, fetcher),
    )
    .unwrap();
    let event = sink.wait_for("clock.event-starting", secs(3));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(event.expect("fired").payload["title"], json!("Planning"));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn the_feed_thread_stops_with_the_handle_and_does_not_wait_for_the_poll() {
    let clock = clock_at(t0());
    let (tx, _rx) = channel::<Event>();
    let fetcher = FakeFetcher::new(Vec::new());
    let (seen, alive) = (fetcher.seen.clone(), fetcher.alive.clone());
    let slow = Parts {
        poll: secs(3600),
        fetcher: Box::new(fetcher),
        ..parts(&clock)
    };
    let handle = launch(armed(starting(URL)), tx, slow).unwrap();
    // Wait for the first read, so the thread is now sleeping until the poll.
    let started = std::time::Instant::now();
    while seen.lock().unwrap().is_empty() && started.elapsed() < secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!seen.lock().unwrap().is_empty());
    drop(handle);
    let stopped = std::time::Instant::now();
    while Arc::strong_count(&alive) > 1 && stopped.elapsed() < secs(1) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(Arc::strong_count(&alive), 1);
}

#[test]
fn rules_without_a_feed_start_no_feed_thread() {
    let clock = clock_at(t0());
    let sink = RecordingSink::new();
    let fetcher = FakeFetcher::new(Vec::new());
    let alive = fetcher.alive.clone();
    let rules = vec![rule("p/i", "clock.interval", json!({ "every_s": 3600 }))];
    let _handle = launch(armed(rules), sink.tx.clone(), feed_parts(&clock, fetcher)).unwrap();
    // Nobody owns the fetcher: it was dropped when `launch` had no use for it.
    assert_eq!(Arc::strong_count(&alive), 1);
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock threads::` passes: 12 tests
- [ ] `cd src-tauri && cargo test -p trigger-clock threads::` passes three times in a row
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes (all earlier tests too; `live` shows 1 ignored)
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `grep -c "reqwest\|println\|eprintln" src-tauri/crates/trigger-clock/src/threads.rs` prints `0`
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Changing `feed.rs`, `os.rs`, `logic.rs`, `lib.rs` or `rule.rs`.
- Logging a fetch failure, or anything with a feed address in it.
- Persisting the `ETag` between runs, or a different poll time.
- Joining the threads on stop, or cancelling a request already in flight.
- Live tests: card T22. Removing `#![allow(dead_code)]`: card T23. README: card T24.

## If stuck

- If the compiler cannot find `uuid` in `threads/tests.rs`, card T15 (which adds the `[dev-dependencies]` entry) was not built: set `status: blocked` and say so.
- If it cannot find `UreqFetcher`, `local_zone`, `read_feed_file`, `feed::expand` or `feed::feed_url`, a slice B card was not built as written: set `status: blocked` and quote the missing name.
- If a test in `threads::` fails once but passes when run again, the machine was busy. Run `cd src-tauri && cargo test -p trigger-clock threads::` three more times. If it fails twice in four runs, set `status: blocked` and quote the failure.
- If pasted code does not compile or clippy objects, check that you pasted it exactly. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
