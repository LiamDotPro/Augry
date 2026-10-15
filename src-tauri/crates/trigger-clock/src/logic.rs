//! Pure decisions: given the wall clock reading, which rules fire and with
//! what payload. No threads, no sleeping, no OS calls: `step` takes `now` as
//! an argument, so a test moves time by passing a different value.
//!
//! The loop around it is designed in the epic's Decisions 4 to 8: the thread
//! wakes at least every `ClockConfig::tick`, reads the wall clock and calls
//! `step`. A gap between two readings of more than `ClockConfig::jump` means
//! the machine slept, because no wait counts time spent asleep.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use trigger_core::{RuleId, Value};

use crate::rule::Parsed;

/// Longest wait between two readings of the wall clock.
pub const TICK: Duration = Duration::from_secs(30);
/// A gap between readings above this means the machine slept (3 ticks).
pub const JUMP: Duration = Duration::from_secs(90);
/// `daily-first-use` fires after the clock has run normally for this long.
pub const NORMAL_FOR: Duration = Duration::from_secs(60);

/// Most due times one rule can fire in a single step. A guard against a
/// pathological rule, not a limit anyone should reach.
const MAX_SLOTS: usize = 10_000;

/// The three durations the loop uses. Tests pass small ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockConfig {
    pub tick: Duration,
    pub jump: Duration,
    pub normal_for: Duration,
}

impl Default for ClockConfig {
    fn default() -> Self {
        ClockConfig {
            tick: TICK,
            jump: JUMP,
            normal_for: NORMAL_FOR,
        }
    }
}

/// A rule this crate is watching for.
#[derive(Debug, Clone)]
pub struct Armed {
    pub id: RuleId,
    pub kind: String,
    pub parsed: Parsed,
}

/// One occurrence of a calendar event inside the window the feed thread
/// expanded. `events::fire_events` reads these from `State::feeds`.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub title: String,
    pub start: SystemTime,
    pub end: SystemTime,
}

/// Which occurrence of which event a rule has fired: the rule, the event's
/// UID and its start in milliseconds (an override that moves an occurrence
/// has a new start, so it counts as a new occurrence).
pub type EventKey = (RuleId, String, u64);

/// Everything `step` remembers between calls.
#[derive(Debug)]
pub struct State {
    pub cfg: ClockConfig,
    /// The wall clock reading of the previous step.
    pub prev_wall: SystemTime,
    /// When the clock last started running normally: process start, or the
    /// end of the last jump or backwards step.
    pub normal_since: SystemTime,
    /// The local date `daily-first-use` last fired for.
    pub last_daily: Option<NaiveDate>,
    /// The next due time of each schedule and interval rule. A rule with no
    /// entry is planned on the next step; an entry of `None` means the rule
    /// will never be due again.
    pub cursors: BTreeMap<RuleId, Option<SystemTime>>,
    /// Occurrences per feed, as last sent by the feed thread, keyed by the
    /// `feed` field exactly as the rule wrote it. The event kinds read this.
    pub feeds: BTreeMap<String, Vec<Occurrence>>,
    /// Event occurrences already fired, each with the time after which it can
    /// no longer be due and is forgotten.
    pub fired_events: BTreeMap<EventKey, SystemTime>,
}

impl State {
    /// Starts the clock at `now`: nothing is due, the clock counts as having
    /// run normally since `now`.
    pub fn new(cfg: ClockConfig, now: SystemTime) -> State {
        State {
            cfg,
            prev_wall: now,
            normal_since: now,
            last_daily: None,
            cursors: BTreeMap::new(),
            feeds: BTreeMap::new(),
            fired_events: BTreeMap::new(),
        }
    }

    /// Forgets every planned due time, so the next step plans from the
    /// current time. Used when the rules change and after a backwards step.
    pub fn replan(&mut self) {
        self.cursors.clear();
    }
}

pub type Payload = BTreeMap<String, Value>;

/// A rule that fired: becomes one `Event`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    pub rule: RuleId,
    pub kind: String,
    pub payload: Payload,
}

pub fn millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn in_zone<Tz: TimeZone>(tz: &Tz, t: SystemTime) -> DateTime<Tz> {
    DateTime::<Utc>::from(t).with_timezone(tz)
}

/// The next due time of a schedule or interval rule strictly after `after`.
/// For a schedule `after` is any instant; for an interval it is the previous
/// due time (or the start). `None` for any other kind, and for a cron
/// expression with no further occurrence.
pub fn next_due<Tz: TimeZone>(armed: &Armed, tz: &Tz, after: SystemTime) -> Option<SystemTime> {
    match armed.kind.as_str() {
        "clock.schedule" => {
            let next = armed
                .parsed
                .cron
                .as_ref()?
                .find_next_occurrence(&in_zone(tz, after), false)
                .ok()?;
            Some(next.into())
        }
        "clock.interval" => Some(after + Duration::from_secs(armed.parsed.when.every_s?)),
        _ => None,
    }
}

/// Every due time from `cursor` up to and including `now`, and the first one
/// after `now`.
fn due_slots<Tz: TimeZone>(
    armed: &Armed,
    tz: &Tz,
    cursor: SystemTime,
    now: SystemTime,
) -> (Vec<SystemTime>, Option<SystemTime>) {
    let mut slots = Vec::new();
    let mut at = Some(cursor);
    while let Some(t) = at.filter(|t| *t <= now) {
        if slots.len() == MAX_SLOTS {
            return (slots, next_due(armed, tz, now));
        }
        slots.push(t);
        at = next_due(armed, tz, t);
    }
    (slots, at)
}

fn due_payload(due: SystemTime, caught_up: bool) -> Payload {
    BTreeMap::from([
        ("due".to_string(), Value::from(millis(due))),
        ("caught_up".to_string(), Value::from(caught_up)),
    ])
}

/// Schedule and interval rules. `jumped` is true when the machine slept.
fn fire_due<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    jumped: bool,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    for armed in rules {
        if !matches!(armed.kind.as_str(), "clock.schedule" | "clock.interval") {
            continue;
        }
        let cursor = *state
            .cursors
            .entry(armed.id.clone())
            .or_insert_with(|| next_due(armed, tz, state.prev_wall));
        let Some(cursor) = cursor else { continue };
        let (slots, next) = due_slots(armed, tz, cursor, now);
        state.cursors.insert(armed.id.clone(), next);
        let fires: Vec<(SystemTime, bool)> = if !jumped {
            slots.into_iter().map(|t| (t, false)).collect()
        } else if armed.parsed.catch_up() {
            slots.last().map(|t| (*t, true)).into_iter().collect()
        } else {
            Vec::new()
        };
        for (due, caught_up) in fires {
            out.push(Fire {
                rule: armed.id.clone(),
                kind: armed.kind.clone(),
                payload: due_payload(due, caught_up),
            });
        }
    }
}

/// `clock.daily-first-use`: fires once per local date, after the clock has run
/// normally for `cfg.normal_for`. A machine that slept all night therefore
/// fires one `normal_for` after waking, not on the jump step itself.
fn fire_daily<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    let today = in_zone(tz, now).date_naive();
    let calm = now.duration_since(state.normal_since).unwrap_or_default() >= state.cfg.normal_for;
    if state.last_daily == Some(today) || !calm {
        return;
    }
    let mut daily = rules
        .iter()
        .filter(|a| a.kind == "clock.daily-first-use")
        .peekable();
    if daily.peek().is_none() {
        return;
    }
    state.last_daily = Some(today);
    for armed in daily {
        out.push(Fire {
            rule: armed.id.clone(),
            kind: armed.kind.clone(),
            payload: BTreeMap::from([("date".to_string(), Value::from(today.to_string()))]),
        });
    }
}

/// The rules due at `now`. `tz` is the zone cron expressions and dates are
/// read in: `&chrono::Local` in the app, a fixed zone in tests.
pub fn step<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
) -> Vec<Fire> {
    let mut out = Vec::new();
    let Ok(gap) = now.duration_since(state.prev_wall) else {
        // The wall clock went backwards (NTP, or the user): fire nothing and
        // plan again from the new time.
        state.prev_wall = now;
        state.normal_since = now;
        state.replan();
        return out;
    };
    let jumped = gap > state.cfg.jump;
    if jumped {
        state.normal_since = now;
    }
    fire_due(state, tz, now, jumped, rules, &mut out);
    fire_daily(state, tz, now, rules, &mut out);
    events::fire_events(state, now, rules, &mut out);
    state.prev_wall = now;
    out
}

/// How long the thread may wait before the next step: until the earliest
/// planned due time, but never longer than `cfg.tick`.
pub fn wait(state: &State, now: SystemTime) -> Duration {
    state
        .cursors
        .values()
        .flatten()
        .map(|t| t.duration_since(now).unwrap_or(Duration::ZERO))
        .min()
        .map_or(state.cfg.tick, |d| d.min(state.cfg.tick))
}

#[cfg(test)]
mod dst_tests;
mod events;

#[cfg(test)]
mod event_tests;

#[cfg(test)]
mod tests;
