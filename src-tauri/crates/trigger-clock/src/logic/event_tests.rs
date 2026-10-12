//! `clock.event-starting` and `clock.event-ended` in `step`.

use super::tests::{armed, at, run, secs};
use super::*;
use serde_json::json;

const FEED: &str = "/cal.ics";

fn occurrence(uid: &str, title: &str, start: SystemTime, minutes: u64) -> Occurrence {
    Occurrence {
        uid: uid.to_string(),
        title: title.to_string(),
        start,
        end: start + secs(minutes * 60),
    }
}

/// Replaces what the feed thread last sent for `FEED`.
fn send(state: &mut State, list: Vec<Occurrence>) {
    state.feeds.insert(FEED.to_string(), list);
}

fn starting(id: &str, extra: serde_json::Value) -> Armed {
    let mut when = json!({ "feed": FEED });
    when.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    armed(id, "clock.event-starting", when)
}

fn standup() -> Occurrence {
    occurrence("standup", "Standup", at(2026, 3, 6, 10, 0, 0), 30)
}

#[test]
fn starting_fires_five_minutes_before_by_default_and_once() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 54, 30), &rules).is_empty());
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].rule, "s");
    assert_eq!(fired[0].kind, "clock.event-starting");
    assert_eq!(fired[0].payload["title"], json!("Standup"));
    assert_eq!(fired[0].payload["feed"], json!(FEED));
    assert_eq!(fired[0].payload.len(), 4);
    assert_eq!(
        fired[0].payload["start"].as_u64().unwrap(),
        millis(at(2026, 3, 6, 10, 0, 0))
    );
    assert_eq!(
        fired[0].payload["end"].as_u64().unwrap(),
        millis(at(2026, 3, 6, 10, 30, 0))
    );
    assert!(run(&mut state, at(2026, 3, 6, 9, 55, 30), &rules).is_empty());
    assert!(run(&mut state, at(2026, 3, 6, 10, 0, 0), &rules).is_empty());
}

#[test]
fn starting_honours_minutes_before() {
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 30, 0));
    send(&mut state, vec![standup()]);
    let early = [starting("s", json!({ "minutes_before": 15 }))];
    assert!(run(&mut state, at(2026, 3, 6, 9, 44, 30), &early).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 45, 0), &early).len(), 1);
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 30, 0));
    send(&mut state, vec![standup()]);
    let on_time = [starting("s", json!({ "minutes_before": 0 }))];
    assert!(run(&mut state, at(2026, 3, 6, 9, 59, 30), &on_time).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 0, 0), &on_time).len(), 1);
}

#[test]
fn a_late_step_still_fires_within_ten_minutes_and_not_after() {
    let rules = [starting("s", json!({}))];
    // Due at 09:55. A step at 10:05:00 is exactly ten minutes late.
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 5, 0), &rules).len(), 1);
    // A step at 10:05:01 is too late, and so is every step after it.
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 5, 1), &rules).is_empty());
    assert!(run(&mut state, at(2026, 3, 6, 10, 5, 31), &rules).is_empty());
}

#[test]
fn waking_from_sleep_fires_an_event_that_became_due_while_asleep() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 0, 0));
    send(&mut state, vec![standup()]);
    run(&mut state, at(2026, 3, 6, 9, 0, 30), &rules);
    // One long jump to 09:58, three minutes after the event became due.
    let fired = run(&mut state, at(2026, 3, 6, 9, 58, 0), &rules);
    assert_eq!(fired.len(), 1);
}

#[test]
fn a_feed_sent_again_does_not_fire_again() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 1);
    send(&mut state, vec![standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 55, 30), &rules).is_empty());
    send(&mut state, vec![standup(), standup()]);
    assert!(run(&mut state, at(2026, 3, 6, 9, 56, 0), &rules).is_empty());
}

#[test]
fn a_feed_that_arrives_after_the_due_time_still_fires() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    assert!(run(&mut state, at(2026, 3, 6, 9, 57, 0), &rules).is_empty());
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 57, 30), &rules).len(), 1);
}

#[test]
fn a_moved_occurrence_fires_again_for_its_new_time() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 1);
    let moved = occurrence("standup", "Standup", at(2026, 3, 6, 11, 0, 0), 30);
    send(&mut state, vec![moved]);
    assert!(run(&mut state, at(2026, 3, 6, 10, 54, 30), &rules).is_empty());
    assert_eq!(run(&mut state, at(2026, 3, 6, 10, 55, 0), &rules).len(), 1);
}

#[test]
fn the_title_glob_picks_which_events_fire() {
    let rules = [starting("s", json!({ "title": "stand*" }))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    let retro = occurrence("retro", "Retro", at(2026, 3, 6, 10, 0, 0), 30);
    send(&mut state, vec![retro, standup()]);
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].payload["title"], json!("Standup"));
}

#[test]
fn every_rule_on_a_feed_fires_and_rules_on_other_feeds_do_not() {
    let other = armed(
        "other",
        "clock.event-starting",
        json!({ "feed": "/other.ics" }),
    );
    let rules = [starting("a", json!({})), starting("b", json!({})), other];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    let fired = run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    let ids: Vec<&str> = fired.iter().map(|f| f.rule.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
}

#[test]
fn two_events_due_together_both_fire() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    let other = occurrence("other", "Other", at(2026, 3, 6, 10, 0, 0), 15);
    send(&mut state, vec![standup(), other]);
    assert_eq!(run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules).len(), 2);
}

#[test]
fn the_memory_of_fired_events_is_forgotten_once_they_cannot_be_due() {
    let rules = [starting("s", json!({}))];
    let mut state = State::new(ClockConfig::default(), at(2026, 3, 6, 9, 50, 0));
    send(&mut state, vec![standup()]);
    run(&mut state, at(2026, 3, 6, 9, 55, 0), &rules);
    assert_eq!(state.fired_events.len(), 1);
    // Due at 09:55, so it can be forgotten after two late windows, 10:15.
    run(&mut state, at(2026, 3, 6, 10, 14, 30), &rules);
    assert_eq!(state.fired_events.len(), 1);
    run(&mut state, at(2026, 3, 6, 10, 15, 30), &rules);
    assert!(state.fired_events.is_empty());
}
