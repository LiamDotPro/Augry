---
id: T22
epic: 007-trigger-clock
title: Add the live tests for interval and feed file events
status: done
depends_on: [T21]
size: S
model: haiku
files:
  - src-tauri/crates/trigger-clock/tests/live.rs
---

## Goal

`tests/live.rs` has three `#[ignore]` tests that start the real source with the real clock and timings: a `clock.interval` rule with the shortest interval fires, and `clock.event-starting` and `clock.event-ended` rules fire from a feed written to a file in a temp folder. They replace the template's placeholder test.

## Read first

- `src-tauri/crates/trigger-clock/tests/live.rs` the template file you replace
- `src-tauri/crates/trigger-fs/tests/live.rs` how the reference crate writes its live tests
- `src-tauri/crates/trigger-clock/src/threads.rs` what `start` does with a feed (read at once, then every 5 minutes)
- `plans/_context/conventions.md` the Rust section on tests that touch the real OS and on temp files

## Steps

1. Replace the whole of `src-tauri/crates/trigger-clock/tests/live.rs` with this. The interval test takes about ten seconds because that is the shortest interval a rule may have. The feed tests finish at once: the feed thread reads the file as soon as it starts, and an event five minutes from starting (or one that ended a minute ago) is already due.

```rust
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
```

2. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock -- --ignored --test-threads=1` passes: 3 tests in `live`, about ten seconds in total. It must pass on this machine.
- [ ] `cd src-tauri && cargo test -p trigger-clock` passes and shows `3 ignored` for `live`
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0
- [ ] `ls "${TMPDIR:-/tmp}" | grep -c trigger-clock-live` prints `0` (the tests removed their folders)
- [ ] `git status --porcelain` lists only paths under `files:` (ignore other card files under `plans/007-trigger-clock/tasks/`)

## Out of scope

- Changing any file under `src/`, or adding a test that sleeps through a real machine sleep.
- Tests against a real URL or a real calendar service.
- Making the interval test faster: 10 seconds is the rule's minimum.
- Editing `README.md`: card T24.

## If stuck

- If a live test fails with a timeout, run it alone: `cd src-tauri && cargo test -p trigger-clock --test live an_interval -- --ignored`. If it passes alone but fails in the full run, write that in the Log and run the full command once more. If it fails again, set `status: blocked` and quote the failure.
- If the `ls` check lists a folder, the tests use `std::env::temp_dir()`, which may not be `$TMPDIR` on this machine: look in that folder instead, and set `status: blocked` only if a `trigger-clock-live-` folder is really left behind.
- If the compiler cannot find `uuid`, `chrono` or `serde_json` in an integration test, quote the error and set `status: blocked`; do not edit `Cargo.toml`.
- If pasted code does not compile or clippy objects, check that you pasted it exactly. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass
