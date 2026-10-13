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
