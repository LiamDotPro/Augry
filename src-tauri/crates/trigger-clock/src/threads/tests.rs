use super::*;
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde_json::json;
use trigger_core::testkit::{rule, FixedClock, RecordingSink};
use trigger_core::{Rule, Source};

use crate::logic::millis;
use crate::ClockSource;

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
        clock: clock.clone(),
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
