//! `clock.event-starting` and `clock.event-ended`: reads the occurrences the
//! feed thread put in `State::feeds` and fires each one once.
//!
//! An occurrence is due from a moment (the start minus `minutes_before`, or
//! the end) until `EVENT_LATE` after it. Inside that window it fires on the
//! first step that sees it and is then remembered, so a step that runs again,
//! or a feed that is sent again, never fires it twice. Outside the window it
//! is skipped: an event that ended an hour ago while the machine slept is not
//! news.

use std::time::{Duration, SystemTime};

use trigger_core::Value;

use super::{millis, Armed, Fire, Payload, State};

/// How long after its due moment an occurrence can still fire.
pub const EVENT_LATE: Duration = Duration::from_secs(600);

fn event_payload(title: &str, start: SystemTime, end: SystemTime, feed: &str) -> Payload {
    Payload::from([
        ("title".to_string(), Value::from(title)),
        ("start".to_string(), Value::from(millis(start))),
        ("end".to_string(), Value::from(millis(end))),
        ("feed".to_string(), Value::from(feed)),
    ])
}

/// Fires the event rules that are due at `now`. `State::feeds` is keyed by the
/// `feed` field exactly as the rule wrote it.
pub(super) fn fire_events(
    state: &mut State,
    now: SystemTime,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    // Forget an occurrence once it can no longer be due.
    state.fired_events.retain(|_, forget_at| *forget_at > now);
    for armed in rules {
        let starting = armed.kind == "clock.event-starting";
        if !starting && armed.kind != "clock.event-ended" {
            continue;
        }
        let Some(feed) = armed.parsed.when.feed.as_deref() else {
            continue;
        };
        let before = Duration::from_secs(u64::from(armed.parsed.minutes_before()) * 60);
        let mut due = Vec::new();
        for occurrence in state.feeds.get(feed).into_iter().flatten() {
            if let Some(title) = &armed.parsed.title {
                if !title.is_match(&occurrence.title) {
                    continue;
                }
            }
            let at = if starting {
                occurrence.start.checked_sub(before)
            } else {
                Some(occurrence.end)
            };
            let Some(at) = at else {
                continue;
            };
            // Not yet due, or too late to matter.
            match now.duration_since(at) {
                Ok(age) if age <= EVENT_LATE => due.push((occurrence.clone(), at)),
                _ => {}
            }
        }
        for (occurrence, at) in due {
            let key = (
                armed.id.clone(),
                occurrence.uid.clone(),
                millis(occurrence.start),
            );
            if state.fired_events.contains_key(&key) {
                continue;
            }
            state.fired_events.insert(key, at + EVENT_LATE * 2);
            out.push(Fire {
                rule: armed.id.clone(),
                kind: armed.kind.clone(),
                payload: event_payload(&occurrence.title, occurrence.start, occurrence.end, feed),
            });
        }
    }
}
