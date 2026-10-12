//! Helpers for testing sources and the hub. Small enough to live in the
//! contract crate, so every source crate can use them as `trigger_core::testkit`.

use crate::{
    Availability, Event, Handle, KindInfo, Rule, RuleError, Sink, Source, SourceError, Value,
};
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// Collects the events a source sends. Give `tx.clone()` to `Source::start`.
pub struct RecordingSink {
    pub tx: Sink,
    rx: Receiver<Event>,
    // Events seen while waiting for a different kind. They are kept, so a
    // later `wait_for` or `drain` still sees them.
    skipped: Mutex<Vec<Event>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        RecordingSink {
            tx,
            rx,
            skipped: Mutex::new(Vec::new()),
        }
    }

    /// Returns the first event of this kind, oldest first, waiting up to
    /// `timeout` for it. Events of other kinds that arrive meanwhile are kept
    /// for later calls. `None` when the time runs out.
    pub fn wait_for(&self, kind: &str, timeout: Duration) -> Option<Event> {
        let mut skipped = self.skipped.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = skipped.iter().position(|e| e.kind == kind) {
            return Some(skipped.remove(i));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(e) if e.kind == kind => return Some(e),
                Ok(e) => skipped.push(e),
                Err(_) => return None,
            }
        }
    }

    /// Everything received and not yet returned, oldest first, without waiting.
    pub fn drain(&self) -> Vec<Event> {
        let mut skipped = self.skipped.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = std::mem::take(&mut *skipped);
        all.extend(self.rx.try_iter());
        all
    }
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a rule. `when` should be a JSON object; anything else gives an empty `when`.
pub fn rule(id: &str, kind: &str, when: Value) -> Rule {
    let when: BTreeMap<String, Value> = match when {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    Rule {
        id: id.to_string(),
        kind: kind.to_string(),
        when,
    }
}

/// A source of "now", so time based logic can be tested without sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Always returns the time inside it. A test changes the time by locking it.
pub struct FixedClock(pub Mutex<SystemTime>);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A source for the hub's tests: records the rules it was started with and
/// fires an event when told to.
pub struct FakeSource {
    pub id: &'static str,
    pub kinds: &'static [KindInfo],
    /// One entry per call to `start`, including calls that fail.
    pub started: Mutex<Vec<Vec<Rule>>>,
    pub availability: Availability,
    /// Set by `start`, used by `fire`.
    pub sink: Mutex<Option<Sink>>,
    /// `start` returns `SourceError::Os` when true.
    pub fail_start: bool,
}

impl FakeSource {
    pub fn new(id: &'static str, kinds: &'static [KindInfo]) -> FakeSource {
        FakeSource {
            id,
            kinds,
            started: Mutex::new(Vec::new()),
            availability: Availability::Available,
            sink: Mutex::new(None),
            fail_start: false,
        }
    }

    /// Sends the event to the sink given to the last `start`. Does nothing if
    /// the source was never started.
    pub fn fire(&self, event: Event) {
        let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = sink.as_ref() {
            let _ = tx.send(event);
        }
    }
}

impl Source for FakeSource {
    fn id(&self) -> &'static str {
        self.id
    }

    fn kinds(&self) -> &'static [KindInfo] {
        self.kinds
    }

    fn availability(&self) -> Availability {
        self.availability.clone()
    }

    fn check(&self, _rule: &Rule) -> Result<(), RuleError> {
        Ok(())
    }

    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError> {
        self.started
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(rules);
        if self.fail_start {
            return Err(SourceError::Os("fake start failure".to_string()));
        }
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        Ok(Handle::new(|| {}))
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use serde_json::json;
    use std::time::SystemTime;

    fn event(kind: &str) -> Event {
        Event {
            rule: "r".into(),
            kind: kind.into(),
            at: SystemTime::now(),
            payload: BTreeMap::new(),
        }
    }

    #[test]
    fn wait_for_returns_a_matching_event() {
        let sink = RecordingSink::new();
        sink.tx.send(event("a")).unwrap();
        let got = sink.wait_for("a", Duration::from_millis(500)).unwrap();
        assert_eq!(got.kind, "a");
    }

    #[test]
    fn wait_for_times_out_with_none() {
        let sink = RecordingSink::new();
        assert!(sink.wait_for("a", Duration::from_millis(50)).is_none());
    }

    #[test]
    fn other_kinds_are_kept_for_drain() {
        let sink = RecordingSink::new();
        sink.tx.send(event("a")).unwrap();
        sink.tx.send(event("b")).unwrap();
        assert!(sink.wait_for("b", Duration::from_millis(500)).is_some());
        let rest = sink.drain();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].kind, "a");
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn rule_builds_from_a_json_object() {
        let r = rule("r1", "fs.x", json!({"path": "/tmp"}));
        assert_eq!(r.id, "r1");
        assert_eq!(r.kind, "fs.x");
        assert_eq!(r.when["path"], json!("/tmp"));
        assert!(rule("r2", "k", json!(null)).when.is_empty());
    }
}

#[cfg(test)]
mod fake_tests {
    use super::*;
    use serde_json::json;
    use std::time::UNIX_EPOCH;

    fn event(kind: &str) -> Event {
        Event {
            rule: "r".into(),
            kind: kind.into(),
            at: UNIX_EPOCH,
            payload: BTreeMap::new(),
        }
    }

    #[test]
    fn fixed_clock_returns_what_is_inside() {
        let clock = FixedClock(Mutex::new(UNIX_EPOCH));
        assert_eq!(clock.now(), UNIX_EPOCH);
        *clock.0.lock().unwrap() = UNIX_EPOCH + Duration::from_secs(5);
        assert_eq!(clock.now(), UNIX_EPOCH + Duration::from_secs(5));
    }

    #[test]
    fn system_clock_is_close_to_now() {
        let before = SystemTime::now();
        assert!(SystemClock.now() >= before);
    }

    #[test]
    fn fake_source_records_rules_and_fires() {
        let source = FakeSource::new("fake", &[]);
        let sink = RecordingSink::new();
        let rules = vec![rule("r1", "fake.x", json!({}))];
        let _handle = source.start(rules.clone(), sink.tx.clone()).unwrap();
        assert_eq!(source.started.lock().unwrap().as_slice(), &[rules]);
        source.fire(event("fake.x"));
        assert!(sink
            .wait_for("fake.x", Duration::from_millis(500))
            .is_some());
    }

    #[test]
    fn fake_source_can_fail_to_start() {
        let mut source = FakeSource::new("fake", &[]);
        source.fail_start = true;
        let sink = RecordingSink::new();
        let result = source.start(vec![], sink.tx.clone());
        assert!(matches!(result, Err(SourceError::Os(_))));
        assert_eq!(source.started.lock().unwrap().len(), 1);
        source.fire(event("fake.x"));
        assert!(sink.drain().is_empty());
    }
}
