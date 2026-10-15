//! Fake engine processes for the supervisor tests.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use trigger_core::testkit::rule;
use trigger_core::RuleId;

use super::*;
use crate::logic::{Armed, Engine, Payload};

/// A real Docker `start` line and a real `die` line (exit code 3), from R01.
pub const START: &str = r#"{"Type":"container","Action":"start","Actor":{"ID":"ef63764e96795c073e6f76c2d2512d8f3b52874bea6b51f94d0c166088379afe","Attributes":{"image":"r01test:1","name":"r01c"}},"scope":"local","time":1791837794,"timeNano":1791837794607003258}"#;
pub const DIE: &str = r#"{"Type":"container","Action":"die","Actor":{"ID":"b2b0cf48d0c03b1e222b1270f8519ef55058954383be0ea0be1251e10ee22244","Attributes":{"exitCode":"3","image":"alpine:3","name":"r01x"}},"scope":"local","time":1791837807,"timeNano":1791837807993939750}"#;

/// What one `spawn` call does.
pub enum Script {
    /// The spawn itself fails.
    Fail,
    /// The child prints these lines and exits.
    Lines(Vec<&'static str>),
}

/// Plays the scripts in order, one per `spawn`. When they run out, `spawn`
/// returns a child that prints nothing and stays alive until it is killed.
pub struct FakeProcess {
    pub engine: Engine,
    scripts: Mutex<VecDeque<Script>>,
    pub spawns: AtomicUsize,
    pub kills: Arc<AtomicUsize>,
}

impl FakeProcess {
    pub fn new(engine: Engine, scripts: Vec<Script>) -> Arc<FakeProcess> {
        Arc::new(FakeProcess {
            engine,
            scripts: Mutex::new(scripts.into()),
            spawns: AtomicUsize::new(0),
            kills: Arc::new(AtomicUsize::new(0)),
        })
    }
}

struct FakeKill {
    gate: Arc<Stop>,
    kills: Arc<AtomicUsize>,
}

impl Kill for FakeKill {
    fn kill(&self) {
        self.kills.fetch_add(1, Ordering::SeqCst);
        self.gate.stop();
    }
}

struct FakeLines {
    pending: VecDeque<&'static str>,
    gate: Arc<Stop>,
    kills: Arc<AtomicUsize>,
}

impl Lines for FakeLines {
    fn next_line(&mut self) -> Option<String> {
        if let Some(line) = self.pending.pop_front() {
            return Some(line.to_string());
        }
        // Out of lines. A child that exits by itself has its gate open, so
        // this returns at once. A child that stays alive waits here until
        // `kill` opens the gate.
        self.gate.wait(Duration::from_secs(30));
        None
    }

    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(FakeKill {
            gate: self.gate.clone(),
            kills: self.kills.clone(),
        })
    }
}

impl EngineProcess for FakeProcess {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn spawn(&self) -> io::Result<Box<dyn Lines>> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        let script = self.scripts.lock().unwrap().pop_front();
        let gate = Arc::new(Stop::default());
        let pending = match script {
            Some(Script::Fail) => return Err(io::Error::other("no such program")),
            Some(Script::Lines(lines)) => {
                // This child exits by itself once its lines are read.
                gate.stop();
                lines.into()
            }
            // Scripts ran out: a child that stays alive until killed.
            None => VecDeque::new(),
        };
        Ok(Box::new(FakeLines {
            pending,
            gate,
            kills: self.kills.clone(),
        }))
    }
}

pub fn armed(kind: &str, when: serde_json::Value) -> Armed {
    let r = rule("p/t", kind, when);
    let parsed = crate::rule::parse(&r).unwrap();
    Armed {
        id: r.id,
        kind: r.kind,
        when: parsed,
    }
}

/// What `emit` was called with, in order.
pub type Seen = Arc<Mutex<Vec<(RuleId, String, Payload)>>>;

pub fn recorder() -> (Seen, Emit) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let emit: Emit = Arc::new(move |id, kind, payload| {
        sink.lock().unwrap().push((id, kind, payload));
    });
    (seen, emit)
}

/// The delays `pause` was asked for. The pause does not sleep.
pub type Delays = Arc<Mutex<Vec<Duration>>>;

pub fn recording_pause() -> (Delays, Pause) {
    let delays: Delays = Arc::new(Mutex::new(Vec::new()));
    let sink = delays.clone();
    let pause: Pause = Arc::new(move |delay, _stop| {
        sink.lock().unwrap().push(delay);
    });
    (delays, pause)
}

/// Polls until `done` is true. Panics after five seconds.
pub fn wait_until(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
