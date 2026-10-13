//! Tests against the real clock, a real file and the real threads. Not run by
//! `cargo test`; run them on a machine of each supported platform with:
//!
//!     cargo test -p trigger-clock -- --ignored --test-threads=1
//!
//! Each test starts the source with the real timings, waits for the event
//! with `RecordingSink::wait_for`, and drops the handle. They take up to
//! about ten seconds each: the shortest interval a rule may have is 10.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde_json::json;
use trigger_clock::ClockSource;
use trigger_core::testkit::{rule, RecordingSink};
use trigger_core::Source;

/// A calendar file in a new temp folder, with one event. Removed on drop.
struct FeedFile {
    dir: PathBuf,
    path: PathBuf,
}

impl FeedFile {
    fn new(title: &str, start: SystemTime, end: SystemTime) -> FeedFile {
        let dir = std::env::temp_dir().join(format!("trigger-clock-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("team.ics");
        let stamp = |t: SystemTime| DateTime::<Utc>::from(t).format("%Y%m%dT%H%M%SZ");
        let text = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//live//EN\r\nBEGIN:VEVENT\r\n\
             UID:live@test\r\nDTSTAMP:20260101T000000Z\r\nDTSTART:{}\r\nDTEND:{}\r\n\
             SUMMARY:{title}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            stamp(start),
            stamp(end)
        );
        std::fs::write(&path, text).unwrap();
        FeedFile { dir, path }
    }

    fn feed(&self) -> String {
        self.path.to_str().unwrap().to_string()
    }
}

impl Drop for FeedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
#[ignore]
fn an_interval_rule_fires_after_its_interval() {
    let sink = RecordingSink::new();
    let started = std::time::Instant::now();
    let _handle = ClockSource
        .start(
            vec![rule("p/t", "clock.interval", json!({ "every_s": 10 }))],
            sink.tx.clone(),
        )
        .expect("start");
    let event = sink
        .wait_for("clock.interval", Duration::from_secs(25))
        .expect("the interval fired");
    assert_eq!(event.rule, "p/t");
    assert_eq!(event.payload["caught_up"], json!(false));
    // Not early: the first firing is one interval after the start.
    assert!(started.elapsed() >= Duration::from_secs(9));
}

#[test]
#[ignore]
fn an_event_starting_soon_in_a_feed_file_fires() {
    let now = SystemTime::now();
    let feed = FeedFile::new(
        "Live planning",
        now + Duration::from_secs(120),
        now + Duration::from_secs(3600),
    );
    let sink = RecordingSink::new();
    let _handle = ClockSource
        .start(
            vec![rule(
                "p/t",
                "clock.event-starting",
                json!({ "feed": feed.feed(), "minutes_before": 5 }),
            )],
            sink.tx.clone(),
        )
        .expect("start");
    let event = sink
        .wait_for("clock.event-starting", Duration::from_secs(10))
        .expect("the event fired");
    assert_eq!(event.payload["title"], json!("Live planning"));
    assert_eq!(event.payload["feed"], json!(feed.feed()));
}

#[test]
#[ignore]
fn an_event_that_just_ended_in_a_feed_file_fires() {
    let now = SystemTime::now();
    let feed = FeedFile::new(
        "Live review",
        now - Duration::from_secs(1800),
        now - Duration::from_secs(60),
    );
    let sink = RecordingSink::new();
    let _handle = ClockSource
        .start(
            vec![rule(
                "p/t",
                "clock.event-ended",
                json!({ "feed": feed.feed(), "title": "live *" }),
            )],
            sink.tx.clone(),
        )
        .expect("start");
    let event = sink
        .wait_for("clock.event-ended", Duration::from_secs(10))
        .expect("the event fired");
    assert_eq!(event.payload["title"], json!("Live review"));
}
